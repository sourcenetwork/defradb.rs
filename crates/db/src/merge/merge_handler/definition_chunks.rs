//! Holds a peer's definition blocks to the `@chunk` rules a local schema follows.

use super::*;
use schema::ChunkSettingsError;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// The rule a chunk collection breaks against its parent, when this node
    /// holds the parent. Without it there is nothing to check yet, and the
    /// chunk collection does nothing until a parent writes into it.
    pub(super) async fn chunk_parent_error(
        &self,
        systemstore: &NamespaceView,
        chunks: &CollectionVersion,
    ) -> Result<Option<ChunkSettingsError>, MergeError> {
        if chunks.chunk_of.is_none() {
            return Ok(None);
        }
        let Some(FieldKind::Relation { collection_id, .. }) = chunks
            .field_by_name(schema::CHUNK_SOURCE_FIELD)
            .map(|field| &field.kind)
        else {
            return Ok(None);
        };
        let held =
            crate::definition::loader::get_collections_by_collection_id(systemstore, collection_id)
                .await
                .map_err(MergeError::Database)?;
        Ok(held
            .iter()
            .find(|parent| !parent.is_placeholder)
            .and_then(|parent| schema::ensure_chunk_parent(parent, chunks).err()))
    }
}

/// The `@chunk` rule `version` breaks. A patch keeps the settings of the
/// version it supersedes; a first version follows the rules the SDL enforces.
pub(super) fn chunk_settings_error(
    previous: Option<&CollectionVersion>,
    version: &CollectionVersion,
) -> Option<ChunkSettingsError> {
    match previous {
        Some(previous) => schema::ensure_chunk_settings_kept(previous, version).err(),
        None => schema::chunk_settings_errors(version).into_iter().next(),
    }
}

/// Rejects the block, so the receiver quarantines it and no version of it is stored.
pub(super) fn reject_chunk_settings(
    version: &CollectionVersion,
    error: &ChunkSettingsError,
) -> MergeOutcome {
    tracing::warn!(
        collection_name = %version.name,
        version_id = %version.version_id,
        chunked_fields = ?version.chunked_fields,
        chunk_of = ?version.chunk_of,
        %error,
        "Synced collection definition rejected: its @chunk settings break the schema rules"
    );
    MergeOutcome::rejected(error.to_string())
}
