//! Whether a filtered replicator takes a document. A `@chunk` chunk follows its parent.

use datastore::NamespaceView;
use document::NormalValue;
use schema::CHUNK_SOURCE_ID_FIELD;
use storage::corekv::Store;

use crate::collection::Collection;
use crate::database::DB;

/// Whether `filters` let the replicator take the document. A chunk has none of
/// its parent's fields, so the parent's filter decides it, by the parent's fields.
pub(super) async fn replica_takes<S>(
    db: &DB<S>,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    collection: &Collection,
    doc_short_id: u64,
    filters: &p2p::ReplicationFilters,
    matcher: &dyn p2p::replicator::ReplicationFilterMatcher,
) -> Result<bool, String>
where
    S: Store,
{
    if let Some(filter) = filters.get(collection.collection_id()) {
        return super::document_matches_filter(
            datastore,
            collection,
            doc_short_id,
            filter,
            matcher,
        )
        .await;
    }
    let Some(parent_col) = crate::chunk::access::parent_collection(db, collection.schema())
        .map_err(|error| format!("failed to load the chunk's parent collection: {error}"))?
    else {
        return Ok(true);
    };
    let Some(filter) = filters.get(parent_col.collection_id()) else {
        return Ok(true);
    };
    let Some(parent_short_id) = parent_of(datastore, systemstore, collection, doc_short_id).await?
    else {
        return Ok(false);
    };
    super::document_matches_filter(datastore, &parent_col, parent_short_id, filter, matcher).await
}

// The stored chunk's parent, read even when the chunk is deleted.
async fn parent_of(
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    chunks_col: &Collection,
    chunk_short_id: u64,
) -> Result<Option<u64>, String> {
    let key = storage::keys::doc_key(chunks_col.collection_id(), chunk_short_id);
    let Some(bytes) = datastore
        .get(&key)
        .await
        .map_err(|error| format!("failed to read chunk for replication filter: {error}"))?
    else {
        return Ok(None);
    };
    let chunk =
        crdt::stored::document_from_storage(&bytes, chunks_col.schema_arc(), chunks_col.crdts())
            .map_err(|error| format!("failed to decode chunk for replication filter: {error}"))?;
    let Some(NormalValue::String(parent_id)) = chunk.get(CHUNK_SOURCE_ID_FIELD) else {
        return Ok(None);
    };
    Ok(crate::docid::map::get_doc_ref(systemstore, parent_id)
        .await
        .map_err(|error| format!("doc-ID mapping lookup failed: {error}"))?
        .map(|parent| parent.doc_short_id))
}
