//! Writing a mutation's local state: the document blob, its indexes, its
//! DocID mappings and, for branchable collections, the collection block.

use super::counter::{apply_local_counter_deltas, init_counter_stores_on_create};
use bytes::Bytes;
use cid::Cid;
use datastore::NamespaceView;
use document::{DocID, Document};
use storage::corekv::Store;

use crate::block::builder::{write_collection_block, BlockResult};
use crate::collection::Collection;
use crate::database::DB;
use crate::index::IndexManager;

pub(crate) async fn write_branchable_collection_block<S: storage::corekv::Store + 'static>(
    db: &crate::database::DB<S>,
    collection_name: &str,
    collection: &Collection,
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc_cid: Cid,
    signing_config: Option<&defra_core::signing::SigningConfig>,
) -> query::error::Result<Option<(Cid, Bytes)>> {
    if !collection.schema().is_branchable {
        return Ok(None);
    }

    let written = write_collection_block(
        blockstore,
        headstore,
        collection.resolved_root_id(),
        collection.version_id(),
        doc_cid,
        signing_config,
    )
    .await
    .map(Some)
    .map_err(|error| {
        query::error::QueryError::execution(format!(
            "failed to write collection block for branchable mutation on collection {}: {}",
            collection_name, error
        ))
    })?;

    // The append just superseded the heads it was built on, and the keys it
    // superseded are reclaimed by a transaction of their own. Every branchable
    // mutation in the crate funnels through here, so this is the one place
    // that has to remember.
    //
    // The reclaiming transaction is opened while the caller's is still open,
    // which is sound: transactions are optimistic so neither blocks the other,
    // and the keys it removes were superseded before the caller began, so it
    // cannot touch anything the caller wrote.
    db.maybe_prune_collection_heads(collection.resolved_root_id())
        .await;

    Ok(written)
}

/// Persist a local UPDATE: apply CRDT field deltas to the authoritative store
/// (the counter RMW, #1021) and then write the document + maintain indexes. These
/// are bundled so no mutator can write a document blob without first advancing the
/// CRDT accumulation store — the single-store invariant is enforced by
/// construction, not by each mutator remembering to call the counter helper.
pub(crate) async fn write_local_update(
    datastore: &NamespaceView,
    collection: &Collection,
    doc: &mut Document,
    doc_short_id: u64,
    index_manager: &IndexManager,
) -> query::error::Result<()> {
    apply_local_counter_deltas(datastore, collection, doc, doc_short_id).await?;
    collection
        .update_with_indexes(datastore, doc, doc_short_id, index_manager)
        .await
        .map_err(|e| match e {
            crate::error::Error::DocumentNotFound(id) => {
                query::error::QueryError::document_not_found(id)
            }
            other => crate::error::index_write_query_error("update", other),
        })
}

/// Persist a local CREATE: write the document + maintain indexes, then seed the
/// CRDT accumulation store for counter fields (#1021). Bundled for the same
/// by-construction reason as `write_local_update`.
pub(crate) async fn write_local_create(
    datastore: &NamespaceView,
    collection: &Collection,
    doc: &Document,
    doc_short_id: u64,
    index_manager: &IndexManager,
) -> query::error::Result<()> {
    collection
        .create_with_indexes(datastore, doc, doc_short_id, index_manager)
        .await
        .map_err(|e| crate::error::index_write_query_error("create", e))?;
    init_counter_stores_on_create(datastore, collection, doc).await
}

/// Persist a local UPDATE WITHOUT the counter read-modify-write (#1044
/// interactive path). The doc blob + indexes are written with the provisional
/// value; the counter RMW (and the blob correction) is deferred to the
/// commit-time finalize. Used only by the interactive `DbDocMutator`; the
/// auto-commit/batch paths keep `write_local_update` (RMW bundled in).
pub(crate) async fn write_local_update_deferred(
    datastore: &NamespaceView,
    collection: &Collection,
    doc: &Document,
    doc_short_id: u64,
    index_manager: &IndexManager,
) -> query::error::Result<()> {
    collection
        .update_with_indexes(datastore, doc, doc_short_id, index_manager)
        .await
        .map_err(|e| match e {
            crate::error::Error::DocumentNotFound(id) => {
                query::error::QueryError::document_not_found(id)
            }
            other => crate::error::index_write_query_error("update", other),
        })
}

/// Register the identity of a freshly created document (Go `save()` isAdd):
/// duplicate-check the derived DocID against the mapping, persist the
/// short-ID <-> DocID mapping, and record block ownership for the genesis
/// composite, field, and encryption blocks. Returns the parsed DocID.
pub(crate) async fn register_created_doc(
    systemstore: &NamespaceView,
    datastore: &NamespaceView,
    collection: &Collection,
    doc_short_id: u64,
    block_result: &BlockResult,
) -> query::error::Result<DocID> {
    let doc_id_str = &block_result.doc_id;

    let existing = crate::docid::map::get_doc_ref(systemstore, doc_id_str)
        .await
        .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
    if let Some(doc_ref) = existing {
        let is_deleted = collection
            .is_deleted(datastore, doc_ref.doc_short_id)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
        if is_deleted {
            return Err(query::error::QueryError::execution(format!(
                "a document with the given ID has been deleted. DocID: {doc_id_str}"
            )));
        }
        return Err(query::error::QueryError::execution(format!(
            "Document with ID {} already exists",
            doc_id_str
        )));
    }

    crate::docid::map::set_doc_id_mapping(
        systemstore,
        collection.resolved_root_id(),
        doc_short_id,
        doc_id_str,
    )
    .await
    .map_err(|e| query::error::QueryError::execution(e.to_string()))?;

    register_block_doc_id_mappings(systemstore, block_result, doc_id_str).await?;
    crate::event::arrivals::record(
        systemstore,
        collection.resolved_root_id(),
        doc_short_id,
        doc_id_str,
    )
    .await
    .map_err(|e| query::error::QueryError::execution(e.to_string()))?;

    DocID::from_string(doc_id_str)
        .map_err(|e| query::error::QueryError::execution(format!("invalid derived DocID: {}", e)))
}

/// Record block ownership (`/d/b/{cid}/{docID}`) for every block produced by
/// a mutation: the composite, each field block, and each encryption block.
pub(crate) async fn register_block_doc_id_mappings(
    systemstore: &NamespaceView,
    block_result: &BlockResult,
    doc_id: &str,
) -> query::error::Result<()> {
    let mut cids = Vec::with_capacity(1 + block_result.field_cids.len());
    cids.push(block_result.cid);
    cids.extend(block_result.field_cids.iter().copied());
    cids.extend(block_result.encryption_cids.iter().copied());
    for cid in cids {
        crate::docid::map::set_block_doc_id_mapping(systemstore, &cid.to_string(), doc_id)
            .await
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
    }
    Ok(())
}

pub(crate) fn ensure_collection_is_active<S: Store>(
    db: &DB<S>,
    collection_name: &str,
    collection: &Collection,
) -> query::error::Result<()> {
    let is_active = db
        .find_collection_by_id(collection.collection_id())
        .map_err(|e| query::error::QueryError::execution(format!("db error: {}", e)))?
        .is_some();

    if is_active {
        Ok(())
    } else {
        Err(query::error::QueryError::collection_not_found(
            collection_name,
        ))
    }
}
