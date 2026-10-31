//! Validators for `@chunk` settings and generated chunk collections.

use super::DefinitionState;
use crate::CollectionVersion;

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
            continue;
        };
        if new_col.chunked_fields != old_col.chunked_fields || new_col.chunk_of != old_col.chunk_of
        {
            errs.push(format!(
                "collection chunk settings cannot be mutated. Collection: {}",
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
