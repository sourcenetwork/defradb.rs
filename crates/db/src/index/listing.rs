//! The indexes a collection lists, as Go's `Collection.ListIndexes` returns
//! them: each with the state of the action that builds it.

use defra_core::{ActionExecution, ActionStatus};
use rapidhash::RapidHashMap;
use schema::{CollectionVersion, FieldKind, IndexDescription, ScalarKind};

/// The indexes of `version` with their build state from `actions`, less the
/// index the node adds for each primary relation on its own. An index
/// without an action record is complete.
pub fn index_listing<'v>(
    version: &'v CollectionVersion,
    actions: &RapidHashMap<u32, ActionExecution>,
) -> Vec<(&'v IndexDescription, ActionExecution)> {
    version
        .indexes
        .iter()
        .filter(|index| !is_hidden_relation_index(version, index))
        .map(|index| {
            let execution = actions
                .get(&index.id)
                .cloned()
                .unwrap_or_else(|| ActionExecution {
                    collection_id: version.collection_id.clone(),
                    subject: index.id.to_string(),
                    status: ActionStatus::COMPLETED,
                    ..Default::default()
                });
            (index, execution)
        })
        .collect()
}

/// Whether `index` is the one the node adds for a primary relation's
/// `_<name>ID` field, which Go does not list.
fn is_hidden_relation_index(version: &CollectionVersion, index: &IndexDescription) -> bool {
    let [field] = index.fields.as_slice() else {
        return false;
    };
    if index.unique {
        return false;
    }
    let Some(relation) = field
        .name
        .strip_prefix('_')
        .and_then(|name| name.strip_suffix("ID"))
    else {
        return false;
    };
    index.name == format!("{}__{relation}ID_ASC", version.name)
        && version.fields.iter().any(|candidate| {
            candidate.name == field.name
                && candidate.relation_name.is_some()
                && candidate.is_primary
                && matches!(candidate.kind, FieldKind::Scalar(ScalarKind::DocID))
        })
}
