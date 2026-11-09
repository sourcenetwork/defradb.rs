//! Creates and deletes single chunk documents in the parent's transaction.

use defra_core::encryption::EncryptionConfig;
use document::{DocID, Document, NormalValue};
use query::error::{QueryError, Result};
use query::mutator::DerivedWrite;
use query::prepared::{PreparedInputKind, PreparedInputRequest};
use schema::{CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD};
use storage::corekv::Store;

use storage::keys::doc_id_index::DocRef;

use crate::block::builder::{write_delete_block_with_inputs, write_document_blocks_with_inputs};
use crate::collection::Collection;
use crate::event::emission::register_update_event_callback;
use crate::write::autocommit::helpers::{register_created_doc, write_local_create};

use super::{json, ChunkCollection, ChunkWrites};

impl<S> ChunkWrites<'_, S>
where
    S: Store + 'static,
{
    // Take the short ID from the memo when there is one, so a retried attempt
    // gives the chunk the same ID, keys and blocks as the first try.
    // Two chunk fields of one parent can hold the same text, so the key names the collection.
    pub(super) async fn short_id(&self, chunks_col: u32, parent: &str, hash: &str) -> Result<u64> {
        let draw = || async {
            self.db
                .next_doc_short_id()
                .await
                .map_err(|e| QueryError::execution(e.to_string()))
        };
        let Some(inputs) = self.prepared_inputs else {
            return draw().await;
        };
        let request = PreparedInputRequest::new(
            PreparedInputKind::Allocation,
            [
                b"chunk-short-id-v2".as_slice(),
                &chunks_col.to_be_bytes(),
                parent.as_bytes(),
                b"/",
                hash.as_bytes(),
            ]
            .concat(),
        );
        let drawn = match inputs.get(&request) {
            Some(drawn) => drawn,
            None => {
                let short_id = draw().await?;
                inputs.insert(request, short_id.to_be_bytes().as_slice())
            }
        };
        <[u8; 8]>::try_from(drawn.as_ref())
            .map(u64::from_be_bytes)
            .map_err(|_| QueryError::execution("invalid prepared chunk short ID"))
    }

    pub(super) async fn create(
        &self,
        target: &ChunkCollection,
        parent: &str,
        text: String,
        hash: String,
        encrypt: bool,
    ) -> Result<DerivedWrite> {
        let collection = &target.collection;
        let mut doc = Document::new(collection.schema_arc());
        let invalid = |e: document::Error| QueryError::execution(format!("chunk document: {e}"));
        doc.set(
            CHUNK_SOURCE_ID_FIELD,
            NormalValue::String(parent.to_string()),
        )
        .map_err(invalid)?;
        doc.set(CHUNK_TEXT_FIELD, NormalValue::String(text))
            .map_err(invalid)?;
        doc.set(CHUNK_HASH_FIELD, NormalValue::String(hash.clone()))
            .map_err(invalid)?;

        let encryption = encrypt.then(|| EncryptionConfig {
            encrypt_doc: true,
            encrypt_fields: Vec::new(),
        });
        let identity = DocRef::new(
            collection.resolved_root_id(),
            self.short_id(collection.resolved_root_id(), parent, &hash)
                .await?,
        );
        let blockstore = self.blockstore()?;
        let headstore = self.headstore()?;
        // With a memo the block writer asks it for keys, so a retry reuses
        // them and nothing remote runs inside the attempt.
        match self.prepared_inputs {
            Some(_) => crate::write::create::seed_identity(self.db, &mut doc, identity),
            None => {
                crate::write::prepare::prepare_keys_with_stores(
                    self.db,
                    &mut doc,
                    identity,
                    None,
                    encryption.as_ref(),
                    Some((&blockstore, &headstore)),
                    None,
                )
                .await?
            }
        }
        let block = write_document_blocks_with_inputs(
            &blockstore,
            &headstore,
            &doc,
            collection.schema(),
            identity,
            None,
            encryption.as_ref(),
            self.context.signing(),
            self.context.batch_session(),
            self.db.kms().as_ref(),
            self.prepared_inputs,
            None,
        )
        .await
        .map_err(|error| {
            error.into_query_error_in(&format!(
                "failed to write chunk blocks on collection {}",
                collection.name()
            ))
        })?;
        let doc_id = register_created_doc(
            None,
            None,
            &target.systemstore,
            &target.datastore,
            collection,
            identity.doc_short_id,
            &block,
        )
        .await?;
        doc.set_id(doc_id.clone());
        write_local_create(
            &target.datastore,
            collection,
            &doc,
            identity.doc_short_id,
            &target.index_manager,
        )
        .await?;
        self.publish(
            collection,
            &doc_id,
            block.cid,
            block.block.clone(),
            json(&doc),
        )?;
        doc.clear_write_preparation();
        Ok(DerivedWrite {
            collection_name: collection.name().to_string(),
            collection_id: collection.collection_id().to_string(),
            doc_id,
            document: Some(doc),
            commit_cid: block.cid,
            commit_block: block.block,
        })
    }

    pub(super) async fn delete(
        &self,
        target: &ChunkCollection,
        doc_short_id: u64,
    ) -> Result<Option<DerivedWrite>> {
        let collection = &target.collection;
        let Some(doc_id) = crate::docid::map::get_doc_id(&target.systemstore, doc_short_id)
            .await
            .map_err(|e| QueryError::execution(e.to_string()))?
        else {
            tracing::warn!(doc_short_id, chunks = %collection.name(), "chunk index names a document with no DocID");
            return Ok(None);
        };
        let doc_id = DocID::from_string(&doc_id)
            .map_err(|e| QueryError::execution(format!("invalid chunk DocID: {e}")))?;
        let before = collection
            .get_with_datastore(&target.datastore, doc_short_id, &doc_id)
            .await
            .map_err(|e| QueryError::execution(format!("chunk read failed: {e}")))?
            .and_then(|doc| json(&doc));
        let existed = collection
            .delete_with_indexes(
                &target.datastore,
                &doc_id,
                doc_short_id,
                &target.index_manager,
            )
            .await
            .map_err(|e| QueryError::execution(format!("chunk delete error: {e}")))?;
        if !existed {
            return Ok(None);
        }
        let blockstore = self.blockstore()?;
        let headstore = self.headstore()?;
        let block = write_delete_block_with_inputs(
            &blockstore,
            &headstore,
            &doc_id.to_string(),
            doc_short_id,
            collection.version_id(),
            self.context.signing(),
            self.context.batch_session(),
            self.prepared_inputs,
            None,
        )
        .await
        .map_err(|error| {
            error.into_query_error_in(&format!(
                "failed to write chunk delete block on collection {}",
                collection.name()
            ))
        })?;
        crate::docid::map::set_block_doc_id_mapping(
            &target.systemstore,
            &block.cid.to_string(),
            &doc_id.to_string(),
        )
        .await
        .map_err(|e| QueryError::execution(e.to_string()))?;
        self.publish(collection, &doc_id, block.cid, block.block.clone(), before)?;
        Ok(Some(DerivedWrite {
            collection_name: collection.name().to_string(),
            collection_id: collection.collection_id().to_string(),
            doc_id,
            document: None,
            commit_cid: block.cid,
            commit_block: block.block,
        }))
    }

    pub(super) fn publish(
        &self,
        collection: &Collection,
        doc_id: &DocID,
        cid: cid::Cid,
        block: bytes::Bytes,
        document_json: Option<serde_json::Value>,
    ) -> Result<()> {
        register_update_event_callback(
            self.txn,
            self.db.event_bus(),
            self.broadcaster,
            self.db.local_commit_release(),
            collection.name().to_string(),
            collection.collection_id().to_string(),
            doc_id.to_string(),
            cid,
            block,
            document_json,
            None,
            None,
        )
        .map_err(|e| {
            QueryError::execution(format!("failed to register chunk update callback: {e}"))
        })
    }

    pub(super) fn blockstore(&self) -> Result<datastore::NamespaceView> {
        self.txn
            .blockstore()
            .map_err(|e| QueryError::execution(format!("failed to get blockstore: {e}")))
    }

    pub(super) fn headstore(&self) -> Result<datastore::NamespaceView> {
        self.txn
            .headstore()
            .map_err(|e| QueryError::execution(format!("failed to get headstore: {e}")))
    }
}
