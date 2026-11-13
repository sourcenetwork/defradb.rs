//! Validators for `@chunk` settings and generated chunk collections.

use super::DefinitionState;
use crate::{chunk_settings_change, chunk_settings_errors, ChunkSettingsError, CollectionVersion};

fn previous<'a>(
    old_state: &'a DefinitionState,
    new_col: &CollectionVersion,
) -> Option<&'a CollectionVersion> {
    old_state
        .collections_by_id
        .get(&new_col.version_id)
        .or_else(|| {
            old_state
                .active_by_collection_id
                .get(&new_col.collection_id)
        })
        .filter(|old| !old.is_placeholder)
}

/// Chunk settings and chunk collections can't change after the schema is
/// added, because existing chunks were cut with them.
pub(super) fn validate_chunks_not_modified(
    new_state: &DefinitionState,
    old_state: &DefinitionState,
) -> Vec<String> {
    let mut errs = Vec::new();
    for new_col in &new_state.collections {
        let Some(old_col) = previous(old_state, new_col) else {
            // Only the SDL can add chunk settings, since it also generates the chunk collection.
            if !new_col.is_placeholder
                && (!new_col.chunked_fields.is_empty() || new_col.chunk_of.is_some())
            {
                errs.push(format!(
                    "chunk settings cannot be added by a patch. Collection: {}",
                    new_col.name
                ));
            }
            continue;
        };
        if let Some(changed) = chunk_settings_change(old_col, new_col) {
            errs.push(format!(
                "collection chunk settings cannot be mutated. Collection: {}, Changed: {changed}",
                new_col.name
            ));
        } else if old_col.is_generated() && new_col.fields != old_col.fields {
            errs.push(format!(
                "a generated chunk collection's fields cannot be mutated. Collection: {}",
                new_col.name
            ));
        }
        for chunked in &old_col.chunked_fields {
            let list = chunked.list_field();
            if new_col.field_by_name(&chunked.field_name).is_none()
                || new_col.field_by_name(&list).is_none()
            {
                errs.push(format!(
                    "a chunked field and its chunk list cannot be removed. Field: {}.{}",
                    new_col.name, chunked.field_name
                ));
            }
        }
    }
    errs
}

/// A view's documents come from its query, so there is no write to cut chunks on.
pub(super) fn chunks_in_views(collections: &[CollectionVersion]) -> Vec<String> {
    collections
        .iter()
        .filter(|col| col.query.is_some() || col.downsample_source.is_some())
        .flat_map(|col| {
            col.chunked_fields.iter().map(move |chunked| {
                ChunkSettingsError::InView {
                    field: format!("{}.{}", col.name, chunked.field_name),
                }
                .to_string()
            })
        })
        .collect()
}

/// New collections follow the `@chunk` rules the SDL enforces, however they were built.
pub(super) fn validate_chunk_settings(
    new_state: &DefinitionState,
    _old_state: &DefinitionState,
) -> Vec<String> {
    new_state
        .collections
        .iter()
        .flat_map(chunk_settings_errors)
        .map(|error| error.to_string())
        .collect()
}
