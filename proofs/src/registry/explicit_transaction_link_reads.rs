use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, datastore, query";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Explicit transaction link reads",
        name: "INV_SecondLinkIsRefused / LIVE_EmptyReadCommits",
        axis: Tla,
        anchor: "crates/database/db/src/index/manager/unique.rs save_local_unique_validated; crates/database/db/src/index/manager/mod.rs on_document_create",
        model_ref: "MC_RelationLinkConflict_{Green,Red_Unvalidated}.cfg",
        tiers: &[Behavioral],
        bound_by: "tools/integration-test/tests/testo/txn/relation_join_read.rs a_writer_of_the_same_link_still_loses_the_commit; crates/database/db/tests/write/disjoint_parts.rs an_indexed_create_does_not_validate_later_scans; crates/database/db/tests/write/disjoint_parts.rs a_unique_indexed_create_does_not_validate_later_scans",
    },
    Property {
        family: "Explicit transaction link reads",
        name: "validated_admits_only_empty / validated_taken_is_refused / unvalidated_admits_both",
        axis: Lean,
        anchor: "crates/database/db/src/index/manager/unique.rs save_local_unique_validated",
        model_ref: "RelationLinkConflict (lake build)",
        tiers: &[Contract],
        bound_by: "tools/integration-test/tests/testo/txn/relation_join_read.rs a_writer_of_the_same_link_still_loses_the_commit; crates/database/db/tests/write/disjoint_parts.rs an_indexed_create_does_not_validate_later_scans; crates/database/db/tests/write/disjoint_parts.rs a_unique_indexed_create_does_not_validate_later_scans",
    },
    Property {
        family: "Explicit transaction link reads",
        name: "INV_JoinReadDecidesNothing / INV_OneBookPerPublisher: a one-to-one join's scan in a query is never validated, while a writer's own uniqueness check on the link is",
        axis: Tla,
        anchor: "crates/engine/query/src/runner/query/nested.rs QueryRunner::execute_nested_select_with_planner; crates/database/db/src/index/manager/unique.rs IndexManager::save_local_unique_validated",
        model_ref: "MC_RelationJoinRead_{Green,Red_JoinScanValidated,Red_OwnEntryUnvalidated}.cfg",
        tiers: &[Behavioral],
        bound_by: "tools/integration-test/tests/testo/txn/relation_join_read.rs a_one_to_one_join_read_does_not_fail_the_commit, a_writer_of_the_same_link_still_loses_the_commit; tools/integration-test/tests/testo/mutation/add/field_kinds/one_to_one_to_one/with_txn.rs transactional_creation_and_linking_of_relational_documents_backward",
    },
];
