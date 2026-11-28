use acp::nac::NodePermission;
use defra_core::RequestContext;
use document::{Document, NormalValue};
use schema::{CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD, CHUNK_TEXT_FIELD};
use sha2::{Digest, Sha256};
use storage::{keys::doc_id_index::DocRef, namespace::Namespace};

use super::{store::StoredVersion, PendingPatch};
use query::{
    error::QueryError,
    prepared::{FirstMiss, PreparedInputMemo},
};
use rapidhash::RapidHashMap;
use storage::corekv::Store;

use crate::chunk::sync::{chunk_document, ChunkCollection, ChunkWrites};
use crate::collection::Collection;
use crate::error::{Error, Result};
use crate::index::IndexManager;
use crate::txn::DbTxn;

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    pub(super) async fn switch_chunk_embeddings(
        &self,
        txn: &mut DbTxn<S>,
        context: &RequestContext,
        patch: PendingPatch,
        inputs: &PreparedInputMemo,
    ) -> Result<StoredVersion> {
        let PendingPatch {
            old_schema,
            mut new_schema,
            ..
        } = patch;
        self.authorize_embedding_switch(txn, context, &old_schema)
            .await?;
        let mut dimensions: Vec<_> = new_schema
            .indexes
            .iter()
            .filter_map(|index| Some((&index.name, index.vector()?.dimensions)))
            .collect();
        dimensions.sort_unstable();
        let settings = serde_json::to_vec(&(
            &old_schema.version_id,
            &new_schema.vector_embeddings,
            dimensions,
        ))
        .map_err(|error| {
            Error::collection_schema_json("serialize chunk embedding settings", error)
        })?;
        new_schema.embedding_generation = Some(hex::encode(Sha256::digest(settings)));
        let mut old = ChunkCollection::open(txn, &old_schema.name).await?;
        let chunks = old
            .collection
            .get_live_with_validated_short_ids(&old.datastore, &old.systemstore)
            .await?;
        let mut next_id = crate::index::manager::index_id_floor(
            &old.datastore,
            old.collection.resolved_root_id(),
        )
        .await?
        .max(
            old_schema
                .indexes
                .iter()
                .map(|index| index.id)
                .max()
                .unwrap_or(0),
        );
        for index in &mut new_schema.indexes {
            if index.is_vector() {
                next_id = next_id
                    .checked_add(1)
                    .ok_or_else(|| Error::InvalidPatch("vector index IDs exhausted".into()))?;
                index.id = next_id;
            }
        }
        let prospective = Collection::new(new_schema.clone(), old.collection.crdts().clone());
        let writes = ChunkWrites {
            db: self,
            txn,
            context,
            prepared_inputs: Some(inputs),
            broadcaster: None,
            follows: None,
        };
        let config = self.options().embedding_config();
        for embedding in &new_schema.vector_embeddings {
            crate::search::embedding::resolve_embedding_config(embedding, &config).map_err(
                |_| {
                    Error::InvalidPatch(format!(
                        "configure the embedding endpoint and selection for {}.{}",
                        new_schema.name, embedding.field_name
                    ))
                },
            )?;
        }

        let mut budget = workers::Budget::new(32);
        let mut first_miss = None;
        let mut replacements = Vec::with_capacity(chunks.len());
        for (id, chunk, _) in chunks {
            budget.spend(1).await;
            let parent = required(&chunk, CHUNK_SOURCE_ID_FIELD)?;
            let text = required(&chunk, CHUNK_TEXT_FIELD)?;
            let hash = required(&chunk, CHUNK_HASH_FIELD)?;
            let mut doc = chunk_document(&prospective, parent, text.to_owned(), hash)?;
            if let Some(miss) = writes.embed(&prospective, &mut doc)? {
                first_miss.get_or_insert(miss);
                continue;
            }
            check_dimensions(&prospective, &doc)?;
            replacements.push((id, parent.to_owned(), hash.to_owned(), doc));
        }
        if let Some(miss) = first_miss {
            return Err(QueryError::NeedsInput(miss).into());
        }

        let blockstore = old.datastore.sibling(Namespace::Blockstore);
        let headstore = old.datastore.sibling(Namespace::Headstore);
        let mut prepared = RapidHashMap::default();
        let mut deletes = FirstMiss::default();
        for (id, parent, hash, doc) in replacements {
            budget.spend(1).await;
            let (encrypted, fields) = crate::block::builder::inherited_encryption_policy(
                &blockstore,
                &headstore,
                DocRef::new(old.collection.resolved_root_id(), id),
                &[CHUNK_TEXT_FIELD.to_owned()],
            )
            .await
            .map_err(Error::other)?;
            let encrypted = encrypted || !fields.is_empty();
            let _ = deletes.keep(writes.delete(&old, id).await)?;
            let entry = prepared.entry((parent, hash)).or_insert((id, doc, false));
            entry.2 |= encrypted;
        }
        deletes.finish(())?;
        for index in old_schema.indexes.iter().filter(|index| index.is_vector()) {
            old.index_manager
                .delete_index(&old.datastore, &index.name)
                .await
                .map_err(|error| Error::InvalidPatch(error.to_string()))?;
            crate::index::manager::retire_index_id(
                &old.datastore,
                old.collection.resolved_root_id(),
                index.id,
            )
            .await?;
        }
        let stored = self
            .store_new_version(txn, new_schema, &old_schema, false, None)
            .await?;
        let collection =
            Collection::load_index_actions(stored.schema.clone(), &old.systemstore).await?;
        let index_manager = IndexManager::from_indexes(
            collection.resolved_root_id(),
            collection.schema(),
            collection.write_indexes(),
            collection.crdts(),
        )
        .map_err(|error| Error::InvalidPatch(error.to_string()))?
        .with_vector_caches(txn.vector_codes())
        .with_drains(txn.deferred_drains());
        let replacement = ChunkCollection::new(
            collection,
            old.datastore.clone(),
            old.systemstore.clone(),
            index_manager,
        )?;
        let writes = ChunkWrites {
            db: self,
            txn,
            context,
            prepared_inputs: Some(inputs),
            broadcaster: None,
            follows: None,
        };
        let mut prepared: Vec<_> = prepared.into_iter().collect();
        prepared.sort_unstable_by_key(|(_, (id, _, _))| *id);
        let mut creates = FirstMiss::default();
        for ((parent, hash), (_, doc, encrypted)) in prepared {
            budget.spend(1).await;
            let _ = creates.keep(
                writes
                    .create(&replacement, &parent, doc, hash, encrypted)
                    .await,
            )?;
        }
        Ok(creates.finish(stored)?)
    }

    async fn authorize_embedding_switch(
        &self,
        txn: &DbTxn<S>,
        context: &RequestContext,
        collection: &schema::CollectionVersion,
    ) -> Result<()> {
        let acpstore = txn.acpstore()?;
        self.check_node_access_in_txn(context, acpstore.clone(), NodePermission::CollectionPatch)
            .await?;
        self.check_node_access_in_txn(context, acpstore.clone(), NodePermission::DocumentRead)
            .await?;
        let source = collection.chunk_of.as_ref().ok_or_else(|| {
            Error::InvalidPatch(format!("{} has no chunk source", collection.name))
        })?;
        let parent = crate::collection::loader::load_collection_from_systemstore(
            &txn.systemstore()?,
            &source.collection,
        )
        .await?
        .ok_or_else(|| Error::CollectionNotFound(source.collection.clone()))?;
        if parent.schema().policy.is_none() && parent.schema().policy_cid.is_none() {
            return Ok(());
        }
        let actor = crate::access::nac::acting_did(context);
        if actor.is_some() && actor == self.node_did() {
            return Ok(());
        }
        if actor.is_some() && context.dac_bypass() {
            return self
                .check_node_access_in_txn(context, acpstore, NodePermission::DacBypass)
                .await;
        }
        Err(Error::NotAuthorized {
            permission: NodePermission::DacBypass.as_str().to_owned(),
        })
    }
}

fn required<'a>(chunk: &'a Document, field: &str) -> Result<&'a str> {
    chunk
        .get(field)
        .and_then(NormalValue::as_str)
        .ok_or_else(|| {
            Error::InvalidPatch(format!("chunk {:?} has no readable {field}", chunk.id()))
        })
}

fn check_dimensions(collection: &Collection, doc: &Document) -> Result<()> {
    for index in collection
        .schema()
        .indexes
        .iter()
        .filter(|index| index.is_vector())
    {
        let Some((field, vector)) = index.fields.first().zip(index.vector()) else {
            continue;
        };
        let Some(length) = doc.get(&field.name).and_then(NormalValue::scalar_array_len) else {
            continue;
        };
        if length != vector.dimensions as usize {
            return Err(Error::InvalidPatch(format!(
                "chunk embedding {}.{} returned {} dimensions, expected {}",
                collection.name(),
                field.name,
                length,
                vector.dimensions
            )));
        }
    }
    Ok(())
}
