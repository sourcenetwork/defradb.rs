//! Validators that keep a collection's indexes naming only its own fields.

use rapidhash::{HashSetExt, RapidHashSet};

use super::DefinitionState;

/// Refuses a patch that removes a field an index of the collection reads.
/// Go refuses it too, as "the given field does not exist".
pub(super) fn validate_indexed_field_not_removed(
    new_state: &DefinitionState,
    old_state: &DefinitionState,
) -> Vec<String> {
    let mut errs = Vec::new();
    for new_col in &new_state.collections {
        let Some(old_col) = old_state
            .collections_by_id
            .get(&new_col.version_id)
            .or_else(|| {
                old_state
                    .active_by_collection_id
                    .get(&new_col.collection_id)
            })
        else {
            continue;
        };
        let kept: RapidHashSet<&str> = new_col.fields.iter().map(|f| f.name.as_str()).collect();
        let mut removed = RapidHashSet::new();
        for field in &old_col.fields {
            if !kept.contains(field.name.as_str()) {
                removed.insert(field.name.as_str());
            }
        }
        if removed.is_empty() {
            continue;
        }
        let plain = new_col.indexes.iter().flat_map(|index| {
            index
                .fields
                .iter()
                .map(move |field| (format!("index '{}'", index.name), field.name.as_str()))
        });
        let encrypted = new_col
            .encrypted_indexes
            .iter()
            .map(|index| ("an encrypted index".to_string(), index.field_name.as_str()));
        for (index, field) in plain.chain(encrypted) {
            if removed.contains(field) {
                errs.push(format!(
                    "cannot remove field '{field}': {index} depends on it, drop the index first. Collection: {}",
                    new_col.name
                ));
            }
        }
    }
    errs
}
