//! When a peer's new chunk document is dropped instead of stored.

use datastore::NamespaceView;
use document::{Document, NormalValue};
use schema::{CHUNK_HASH_FIELD, CHUNK_SOURCE_CID_FIELD, CHUNK_SOURCE_ID_FIELD};
use storage::corekv::Store;

use crate::chunk::peer_delete::{parent_state, ParentState};
use crate::chunk::sync::ChunkCollection;
use crate::collection::Collection;
use crate::database::DB;
use crate::error::{Error, Result};
use crate::index::IndexManager;

/// Why a peer's new, unencrypted chunk is dropped, or `None` when it is stored.
// A merge makes any dropped chunk a later text needs, so dropping loses nothing.
pub(crate) async fn create_is_stale<S>(
    db: &DB<S>,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    chunks_col: &Collection,
    chunk: &Document,
) -> Result<Option<&'static str>>
where
    S: Store,
{
    let (Some(NormalValue::String(parent_id)), Some(NormalValue::String(hash))) = (
        chunk.get(CHUNK_SOURCE_ID_FIELD),
        chunk.get(CHUNK_HASH_FIELD),
    ) else {
        return Ok(None);
    };
    let state = parent_state(db, datastore, systemstore, chunks_col, parent_id, hash).await?;
    if state == ParentState::Deleted {
        return Ok(Some("the chunk's parent is deleted"));
    }
    let index_manager = IndexManager::from_collection(
        chunks_col.resolved_root_id(),
        chunks_col.schema(),
        chunks_col.crdts(),
    )?;
    let target = ChunkCollection::new(
        chunks_col.clone(),
        datastore.clone(),
        systemstore.clone(),
        index_manager,
    )
    .map_err(|e| Error::Other(e.to_string()))?;
    let other = |e: query::error::QueryError| Error::Other(e.to_string());
    let source = chunk
        .get(CHUNK_SOURCE_CID_FIELD)
        .and_then(NormalValue::as_str);
    // A chunk of a commit not merged here waits for it, so it is kept.
    if target.waits(parent_id, source, None).await.map_err(other)? {
        return Ok(None);
    }
    let ParentState::Live(Some(uses)) = state else {
        return Ok(None);
    };
    if !uses {
        return Ok(Some("the chunk's parent does not use it"));
    }
    let mut stored = target.stored_of(parent_id).await.map_err(other)?;
    for held in stored.remove(hash).into_iter().flatten() {
        if !target
            .waits(parent_id, held.source.as_deref(), None)
            .await
            .map_err(other)?
        {
            return Ok(Some("the chunk's parent already holds it"));
        }
    }
    Ok(None)
}
