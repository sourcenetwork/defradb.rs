//! An update of a document its caller selected before the update's transaction.

use document::Document;
use query::update_parts::Run;
use rapidhash::RapidHashSet;

use crate::collection::Collection;

/// The fields the update decides on: those this run writes, plus those the caller compared.
pub(super) fn dependencies(
    collection: &Collection,
    run: Run,
    modified: &RapidHashSet<String>,
    input: &Document,
    expected_fields: Option<&RapidHashSet<String>>,
) -> RapidHashSet<String> {
    let mut dependencies = run.written(
        collection.crdts(),
        collection.schema(),
        collection.write_indexes(),
        modified,
        |field| input.pending_op(field).is_some(),
    );
    dependencies.extend(expected_fields.into_iter().flatten().cloned());
    dependencies
}

/// Whether `current` is still the document the caller selected, on `compared`
/// or, without it, on every field.
pub(super) fn unchanged(
    expected: &Document,
    compared: Option<&RapidHashSet<String>>,
    current: &Document,
) -> bool {
    let values_match = match compared {
        Some(fields) => fields
            .iter()
            .all(|field| expected.get(field) == current.get(field)),
        None => {
            expected.values().len() == current.values().len()
                && expected
                    .values()
                    .iter()
                    .all(|(name, value)| current.get(name) == Some(value.value()))
        }
    };
    values_match && expected.id() == current.id() && expected.is_deleted() == current.is_deleted()
}
