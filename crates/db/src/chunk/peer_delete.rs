//! When a peer's delete of a chunk may merge.

use datastore::NamespaceView;
use document::{DocID, NormalValue};
use schema::CHUNK_SOURCE_ID_FIELD;
use storage::corekv::Store;

use crate::collection::Collection;
use crate::database::DB;
use crate::error::Result;

/// Why a peer's delete of a chunk must wait, or `None` when it may merge.
/// Chunks go only with their parent, and a later edit may use an unlisted one again.
pub(crate) async fn delete_must_wait<S>(
    db: &DB<S>,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    chunks_col: &Collection,
    chunk_short_id: u64,
    chunk_id: &DocID,
) -> Result<Option<&'static str>>
where
    S: Store,
{
    // A delete resolves its chunk through the chunk's first block, which merges
    // before it, so a chunk missing here stored nothing this node can read.
    let Some(chunk) = chunks_col
        .get_with_datastore(datastore, chunk_short_id, chunk_id)
        .await?
    else {
        return Ok(None);
    };
    // A chunk this node can't read shows nothing, so its delete hides nothing.
    let Some(NormalValue::String(parent_id)) = chunk.get(CHUNK_SOURCE_ID_FIELD) else {
        return Ok(None);
    };
    let Some(parent_col) = super::access::parent_collection(db, chunks_col.schema())? else {
        return Ok(Some("the chunk's parent collection is not known here"));
    };
    let Some(parent_ref) = crate::docid::map::get_doc_ref(systemstore, parent_id).await? else {
        return Ok(Some("the chunk's parent has not arrived"));
    };
    if parent_col
        .is_deleted(datastore, parent_ref.doc_short_id)
        .await?
    {
        return Ok(None);
    }
    // A merge can map the parent's ID before it stores the parent.
    let stored = parent_col
        .get_with_datastore(
            datastore,
            parent_ref.doc_short_id,
            &DocID::from_string(parent_id)?,
        )
        .await?
        .is_some();
    Ok(Some(if stored {
        "the chunk's parent is live"
    } else {
        "the chunk's parent has not arrived"
    }))
}
