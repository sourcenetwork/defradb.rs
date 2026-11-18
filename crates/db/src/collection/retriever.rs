//! Resolve a collection from a document ID via the headstore.
//!
//! Ports Go's `internal/db/collection_retriever.go::RetrieveCollectionFromDocID`.
//! The headstore is keyed by doc_id (`HeadstorePriorityKey::document_prefix`),
//! so finding the doc's first head block is O(1) — no scan over all
//! collections. We then read `collection_version_id` off the block's delta and
//! resolve the collection via the systemstore.

use std::sync::Arc;

use cid::Cid;
use storage::corekv::Store;
use storage::keys::HeadstorePriorityKey;
use storage::IterOptions;

use datastore::NamespaceView;
use schema::CollectionVersion;

use crate::database::DB;
use crate::error::{Error, Result};

/// Subset of a collection's policy metadata sufficient for KMS access-control
/// checks. Returned by `resolve_collection_from_doc_id`.
#[derive(Debug, Clone)]
pub struct DocCollectionInfo {
    /// The collection id (for tracing; not used in the policy check itself).
    pub collection_id: String,
    /// Policy id under which to check the DAC permission.
    pub policy_id: String,
    /// Resource name within the policy.
    pub resource_name: String,
    /// Whether the collection is branchable.
    pub is_branchable: bool,
    /// Its `@chunk` collections, whose keys follow this collection's documents.
    pub chunk_collection_ids: Vec<String>,
}

/// Resolve the collection metadata for a given document.
///
/// Returns `Ok(None)` when:
/// - The doc has no head blocks in the local headstore (unknown doc).
/// - The doc's first head block is not in the local blockstore.
/// - The block's delta has no collection_version_id (e.g. definition blocks).
/// - The collection_version_id does not resolve to a collection in the systemstore.
/// - The collection has no policy configured (`collection.policy == None`).
///
/// Mirrors Go's `RetrieveCollectionFromDocID` — same lookup chain via the
/// headstore index, no scan.
pub async fn resolve_collection_from_doc_id<S: Store>(
    db: &Arc<DB<S>>,
    doc_id: &str,
) -> Result<Option<DocCollectionInfo>> {
    // The transaction is held as an owned local across the awaits below. The
    // store handles (`txn.headstore()` / `.blockstore()` / `.systemstore()`)
    // are owned `NamespaceView`s, so `&DbTxn` is only borrowed momentarily and
    // never held across an await. This keeps the future `Send`: a held
    // `&DbTxn` across an await would make it `!Send` (DbTxn is `!Sync` due to
    // its callback registry), which breaks `kms::DocCollectionLookup` whose
    // futures must be `Send`. The read-only txn is discarded on every return
    // path (matches schema_loader.rs and Go's `defer txn.Discard()`).
    let txn = db.new_txn(true).await?;
    let stores = (txn.headstore()?, txn.blockstore()?, txn.systemstore()?);
    let found = stored_doc_collection(&stores.0, &stores.1, &stores.2, doc_id).await;
    drop(stores);
    let _ = txn.discard();
    Ok(found?.and_then(|collection| {
        collection.policy.clone().map(|policy| DocCollectionInfo {
            collection_id: collection.collection_id.clone(),
            policy_id: policy.id,
            resource_name: policy.resource_name,
            is_branchable: collection.is_branchable,
            chunk_collection_ids: collection
                .chunked_fields
                .iter()
                .filter_map(|chunked| {
                    db.get_collection(&chunked.collection(&collection.name))
                        .ok()
                        .flatten()
                })
                .map(|chunks| chunks.collection_id().to_string())
                .collect(),
        })
    }))
}

/// The collection version `doc_id`'s first head block names.
pub(crate) async fn stored_doc_collection(
    headstore: &NamespaceView,
    blockstore: &NamespaceView,
    systemstore: &NamespaceView,
    doc_id: &str,
) -> Result<Option<CollectionVersion>> {
    let Some(doc_ref) = crate::docid::map::get_doc_ref(systemstore, doc_id).await? else {
        return Ok(None);
    };
    let doc_short_id = doc_ref.doc_short_id;
    let prefix = HeadstorePriorityKey::document_prefix(doc_short_id);
    let opts = IterOptions::new().with_prefix(prefix);
    let mut iter = headstore.iterator(opts).await.map_err(Error::Storage)?;
    let cid_offset = HeadstorePriorityKey::cid_offset(doc_short_id);
    let pair = iter.next().await.map_err(Error::Storage);
    let _ = iter.close().await;
    let Some(cid_bytes) = pair?.and_then(|pair| pair.key.get(cid_offset..).map(<[u8]>::to_vec))
    else {
        return Ok(None);
    };
    let cid = Cid::try_from(cid_bytes.as_slice())
        .map_err(|e| Error::Serialization(format!("decode head cid: {e}")))?;
    let Some(block_bytes) = blockstore
        .get(&cid.to_bytes())
        .await
        .map_err(Error::Storage)?
    else {
        return Ok(None);
    };
    let block = defra_core::block::Block::from_dag_cbor(&block_bytes)
        .map_err(|e| Error::Serialization(format!("decode block: {e}")))?;
    let Some(collection_version_id) = block.delta.collection_version_id() else {
        return Ok(None);
    };
    crate::definition::loader::get_collection_by_version_id(systemstore, collection_version_id)
        .await
}
