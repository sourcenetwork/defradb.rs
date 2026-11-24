use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Explicit transaction link reads",
        name: "INV_SecondLinkIsRefused / LIVE_EmptyReadCommits",
        axis: Tla,
        anchor: "crates/database/db/src/index/manager/unique.rs save_local_unique_validated; crates/database/db/src/index/manager/mod.rs on_document_create; crates/engine/query/src/runner/query/nested.rs has_one_to_one_relation_select",
        model_ref: "MC_RelationLinkConflict_{Green,Red_Unvalidated}.cfg",
        tiers: &[Behavioral],
        bound_by: "tools/integration-test/tests/testo/mutation/add/field_kinds/one_to_one_to_one/with_txn.rs transactional_creation_and_linking_of_relational_documents_backward; crates/database/db/tests/write/disjoint_parts.rs an_indexed_create_does_not_validate_later_scans; crates/database/db/tests/write/disjoint_parts.rs a_unique_indexed_create_does_not_validate_later_scans",
    },
    Property {
        family: "Explicit transaction link reads",
        name: "validated_admits_only_empty / validated_taken_is_refused / unvalidated_admits_both",
        axis: Lean,
        anchor: "crates/database/db/src/index/manager/unique.rs save_local_unique_validated; crates/engine/query/src/runner/query/nested.rs has_one_to_one_relation_select",
        model_ref: "RelationLinkConflict (lake build)",
        tiers: &[Contract],
        bound_by: "tools/integration-test/tests/testo/mutation/add/field_kinds/one_to_one_to_one/with_txn.rs transactional_creation_and_linking_of_relational_documents_backward; crates/database/db/tests/write/disjoint_parts.rs an_indexed_create_does_not_validate_later_scans; crates/database/db/tests/write/disjoint_parts.rs a_unique_indexed_create_does_not_validate_later_scans",
    },
];
