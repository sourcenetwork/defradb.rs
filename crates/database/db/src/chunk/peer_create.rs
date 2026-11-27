//! When a peer's new chunk document is dropped instead of stored.

use datastore::NamespaceView;
use document::{Document, NormalValue};
use schema::{CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD};
use storage::corekv::Store;

use crate::chunk::peer_delete::{parent_state, ParentState};
use crate::chunk::sync::ChunkCollection;
use crate::collection::Collection;
use crate::database::DB;
use crate::error::{Error, Result};
use crate::index::IndexManager;

/// Why a peer's new, unencrypted chunk is dropped, or `None` when it is stored.
/// Its live parent here does not use it or already holds it. The merge of a
/// later text that uses it builds it, so dropping it loses nothing.
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
    let ParentState::Live(Some(uses)) =
        parent_state(db, datastore, systemstore, chunks_col, parent_id, hash).await?
    else {
        return Ok(None);
    };
    if !uses {
        return Ok(Some("the chunk's parent does not use it"));
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
    let held = target
        .stored_of(parent_id)
        .await
        .map_err(|e| Error::Other(e.to_string()))?
        .contains_key(hash);
    Ok(held.then_some("the chunk's parent already holds it"))
}
