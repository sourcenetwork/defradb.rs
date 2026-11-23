//! Validators that keep a collection's indexes naming only its own fields.

use rapidhash::RapidHashSet;

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
        let fulltext = new_col
            .fulltext_indexes
            .iter()
            .map(|index| ("a full-text index".to_string(), index.field_name.as_str()));
        let readers: Vec<(String, &str)> = plain.chain(encrypted).chain(fulltext).collect();
        // One error per removed field, naming the first index that reads it.
        for field in &old_col.fields {
            let field = field.name.as_str();
            if kept.contains(field) {
                continue;
            }
            if let Some((index, _)) = readers.iter().find(|(_, read)| *read == field) {
                errs.push(format!(
                    "cannot remove field '{field}': {index} depends on it, drop the index first. Collection: {}",
                    new_col.name
                ));
            }
        }
    }
    errs
}
