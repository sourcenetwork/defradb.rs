//! When a peer's delete of a chunk may merge.

use datastore::NamespaceView;
use document::{DocID, NormalValue};
use schema::{CHUNK_HASH_FIELD, CHUNK_SOURCE_CID_FIELD, CHUNK_SOURCE_ID_FIELD};
use storage::corekv::Store;

use crate::chunk::split::split;
use crate::collection::Collection;
use crate::database::DB;
use crate::error::Result;

/// What a chunk's parent is on this node.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParentState {
    /// The chunk collection's parent collection is not known here.
    UnknownCollection,
    /// The parent has not arrived, or is mapped but not stored yet.
    NotArrived,
    Deleted,
    /// Live, and whether its text uses the chunk. `None` when the text can't be read here.
    Live(Option<bool>),
}

/// What a peer's delete of a chunk does here.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DeleteVerdict {
    Merge,
    /// It waits, for this reason.
    Wait(&'static str),
    /// Nothing here to delete, so it is dropped and this node may still make the chunk.
    Drop(&'static str),
}

/// Whether a peer's delete of a chunk merges, waits or is dropped.
// Chunks go only with their parent, so a chunk its text uses stays.
pub(crate) async fn delete_verdict<S>(
    db: &DB<S>,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    chunks_col: &Collection,
    chunk_short_id: u64,
    chunk_id: &DocID,
) -> Result<DeleteVerdict>
where
    S: Store,
{
    let Some(chunk) = chunks_col
        .get_with_datastore(datastore, chunk_short_id, chunk_id)
        .await?
    else {
        return Ok(DeleteVerdict::Drop("the chunk is not stored here"));
    };
    // A chunk this node can't read shows nothing, so its delete hides nothing.
    let (Some(NormalValue::String(parent_id)), Some(NormalValue::String(hash))) = (
        chunk.get(CHUNK_SOURCE_ID_FIELD),
        chunk.get(CHUNK_HASH_FIELD),
    ) else {
        return Ok(DeleteVerdict::Merge);
    };
    if let Some(NormalValue::String(source)) = chunk.get(CHUNK_SOURCE_CID_FIELD) {
        if !crate::chunk::source::is_merged(systemstore, parent_id, source, None).await? {
            return Ok(DeleteVerdict::Wait(
                "the chunk's parent commit has not merged here",
            ));
        }
    }
    Ok(
        match parent_state(db, datastore, systemstore, chunks_col, parent_id, hash).await? {
            ParentState::UnknownCollection => {
                DeleteVerdict::Wait("the chunk's parent collection is not known here")
            }
            ParentState::NotArrived => DeleteVerdict::Wait("the chunk's parent has not arrived"),
            ParentState::Deleted | ParentState::Live(Some(false)) => DeleteVerdict::Merge,
            ParentState::Live(Some(true)) => {
                DeleteVerdict::Wait("the chunk's parent is live and uses it")
            }
            ParentState::Live(None) => DeleteVerdict::Wait("the chunk's parent is live"),
        },
    )
}

/// The state of the parent `parent_id` of a chunk of `chunks_col` with `hash`.
/// It reads only the chunked field, so an edit of another field does not conflict.
pub(crate) async fn parent_state<S>(
    db: &DB<S>,
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    chunks_col: &Collection,
    parent_id: &str,
    hash: &str,
) -> Result<ParentState>
where
    S: Store,
{
    let Some(parent_col) = super::access::parent_collection(db, chunks_col.schema())? else {
        return Ok(ParentState::UnknownCollection);
    };
    let Some(parent_ref) = crate::docid::map::get_doc_ref(systemstore, parent_id).await? else {
        return Ok(ParentState::NotArrived);
    };
    if parent_col
        .is_deleted(datastore, parent_ref.doc_short_id)
        .await?
    {
        return Ok(ParentState::Deleted);
    }
    let Some(chunked) = chunks_col
        .schema()
        .chunk_of
        .as_ref()
        .and_then(|source| parent_col.schema().chunked_field(&source.field_name))
    else {
        return Ok(ParentState::Live(None));
    };
    let fields = std::iter::once(chunked.field_name.clone()).collect();
    // A merge can map the parent's ID before it stores the parent.
    let Some(parent) = parent_col
        .get_with_datastore_fields(
            datastore,
            parent_ref.doc_short_id,
            &DocID::from_string(parent_id)?,
            &fields,
        )
        .await?
    else {
        return Ok(ParentState::NotArrived);
    };
    let uses = match parent.get(&chunked.field_name) {
        Some(NormalValue::String(text)) => {
            let config = crate::chunk::chunk_config(parent_col.name(), chunked)?;
            Some(split(text, &config).iter().any(|chunk| chunk.hash == hash))
        }
        None | Some(NormalValue::Null) => Some(false),
        Some(_) => None,
    };
    Ok(ParentState::Live(uses))
}
