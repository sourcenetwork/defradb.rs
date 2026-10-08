//! Document creation: the one path every mutator's create goes through.

use bytes::Bytes;
use cid::Cid;
use datastore::NamespaceView;
use document::{DocID, Document};
use query::error::{QueryError, Result};
use storage::corekv::Store;

use crate::block::builder::{
    compute_document_blocks, insert_computed_blocks, resolve_document_keys, ComputedBlocks,
    DocStorageIdentity, DocumentKeys,
};
use crate::collection::Collection;
use crate::database::DB;
use crate::index::IndexManager;
use crate::txn::DbTxn;
use crate::write::persist::{
    register_created_doc, write_branchable_collection_block, write_local_create,
};
use defra_core::encryption::get_encryption_config;
use defra_core::signing::{get_signing_config, SigningConfig};

/// The transaction store views a create writes through.
///
/// Taken from the transaction up front because `DbTxn` is not `Sync`; the
/// views must be dropped before the transaction's owner commits it.
pub(crate) struct TxnStores {
    pub datastore: NamespaceView,
    pub systemstore: NamespaceView,
    pub blockstore: NamespaceView,
    pub headstore: NamespaceView,
}

impl TxnStores {
    pub fn of<S: Store>(txn: &DbTxn<S>) -> Result<Self> {
        let store_error = |e: crate::error::Error| QueryError::execution(e.to_string());
        Ok(Self {
            datastore: txn.datastore().map_err(store_error)?,
            systemstore: txn.systemstore().map_err(store_error)?,
            blockstore: txn.blockstore().map_err(store_error)?,
            headstore: txn.headstore().map_err(store_error)?,
        })
    }
}

/// A document written by [`create_documents`], with the blocks a mutator
/// needs to publish once its transaction commits.
pub(crate) struct CreatedDoc {
    pub doc_id: DocID,
    pub doc: Document,
    pub cid: Cid,
    pub block: Bytes,
    pub collection_block: Option<(Cid, Bytes)>,
}

/// Create `docs` in `collection` through the transaction's `stores`.
///
/// Every document is fully formed in memory (embeddings included) and every
/// block computed before anything is stored, so no field block or head is
/// written until the whole document, and its DocID, is known. The caller owns
/// the transaction: it decides when to commit, and holds whatever locks its
/// mutation scope needs.
pub(crate) async fn create_documents<S: Store + 'static>(
    db: &DB<S>,
    stores: &TxnStores,
    collection_name: &str,
    collection: &Collection,
    index_manager: &IndexManager,
    docs: Vec<Document>,
) -> Result<Vec<CreatedDoc>> {
    let store_error = |e: crate::error::Error| QueryError::execution(e.to_string());
    let TxnStores {
        datastore,
        systemstore,
        blockstore,
        headstore,
    } = stores;

    let schema_version_id = collection.version_id();
    let enc_config = get_encryption_config();
    let sign_config = get_signing_config();
    let kms = db.kms();
    let embedding_config = db.options().embedding_config();

    let mut prepared = Vec::with_capacity(docs.len());
    for mut doc in docs {
        crate::search::set_embedding(
            &collection.schema().vector_embeddings,
            &mut doc,
            true,
            None,
            &embedding_config,
        )
        .await
        .map_err(|e| QueryError::execution(format!("embedding error: {}", e)))?;

        db.validate_downsample_write(datastore, systemstore, collection.schema(), &doc, None)
            .await
            .map_err(store_error)?;

        let doc_short_id = db.next_doc_short_id().await.map_err(store_error)?;
        let identity = DocStorageIdentity::new(collection.resolved_root_id(), doc_short_id);
        let keys = resolve_document_keys(&doc, identity, enc_config.as_ref(), kms.as_ref())
            .await
            .map_err(|e| block_error(collection_name, e))?;
        prepared.push((doc, identity, keys));
    }

    let computed = compute_all(&prepared, schema_version_id, sign_config.as_ref())
        .await
        .map_err(|e| block_error(collection_name, e))?;

    let mut created = Vec::with_capacity(prepared.len());
    for ((mut doc, identity, _), blocks) in prepared.into_iter().zip(computed) {
        insert_computed_blocks(blockstore, headstore, &blocks)
            .await
            .map_err(|e| block_error(collection_name, e))?;
        let result = blocks.block_result;

        let doc_id = register_created_doc(
            systemstore,
            datastore,
            collection,
            identity.doc_short_id,
            &result,
        )
        .await?;
        doc.set_id(doc_id.clone());

        write_local_create(
            datastore,
            collection,
            &doc,
            identity.doc_short_id,
            index_manager,
        )
        .await?;

        let collection_block = write_branchable_collection_block(
            db,
            collection_name,
            collection,
            blockstore,
            headstore,
            result.cid,
            sign_config.as_ref(),
        )
        .await?;

        if let Some(session_key) = defra_core::batch_signing::get_batch_session_key() {
            for cid in result.field_cids.iter().chain([&result.cid]) {
                defra_core::batch_signing::batch_collect_cid(&session_key, *cid);
            }
        }

        created.push(CreatedDoc {
            doc_id,
            doc,
            cid: result.cid,
            block: result.block,
            collection_block,
        });
    }
    Ok(created)
}

fn block_error(collection_name: &str, error: String) -> QueryError {
    QueryError::execution(format!(
        "failed to write document blocks for create on collection {}: {}",
        collection_name, error
    ))
}

/// Compute every document's blocks, in parallel when there is more than one
/// and the runtime can spare blocking threads.
async fn compute_all(
    prepared: &[(Document, DocStorageIdentity, DocumentKeys)],
    schema_version_id: &str,
    sign_config: Option<&SigningConfig>,
) -> std::result::Result<Vec<ComputedBlocks>, String> {
    #[cfg(feature = "native")]
    if prepared.len() > 1 {
        let tasks = prepared.iter().map(|(doc, identity, keys)| {
            let (doc, identity, keys) = (doc.clone(), *identity, keys.clone());
            let schema_version_id = schema_version_id.to_string();
            let sign_config = sign_config.cloned();
            tokio::task::spawn_blocking(move || {
                compute_document_blocks(
                    &doc,
                    &schema_version_id,
                    identity,
                    &keys,
                    sign_config.as_ref(),
                )
            })
        });
        return futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|joined| joined.map_err(|e| format!("block computation task panicked: {e}"))?)
            .collect();
    }

    prepared
        .iter()
        .map(|(doc, identity, keys)| {
            compute_document_blocks(doc, schema_version_id, *identity, keys, sign_config)
        })
        .collect()
}
