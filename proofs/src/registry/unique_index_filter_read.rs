use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "storage, db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Unique index filter read",
        name: "INV_FilterReadDecidesNothing / INV_Unique: a unique-index filter read is never validated, while a writer's own uniqueness check is",
        axis: Tla,
        anchor: "crates/engine/storage/src/index/eq_iterator.rs ExactMatchIterator::new_unique; crates/engine/storage/src/index/unique.rs UniqueIndex::save",
        model_ref: "MC_UniqueFilterRead_{Green,Red_FilterReadValidated,Red_FilterReadValidatedNil,Red_OwnEntryUnvalidated}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/txn/unique_filter_read.rs a_unique_eq_filter_read_does_not_fail_the_commit, a_nil_writers_unique_filter_read_does_not_fail_the_commit, a_writer_of_the_filtered_value_still_loses_the_commit; tools/integration-test/tests/testo/mutation/add/field_kinds/one_to_one_to_one/with_txn.rs transactional_creation_and_linking_of_relational_documents_forward",
    },
];
