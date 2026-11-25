use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Missing relation filters",
        name: "Safe: a missing child passes only with an ID condition and matching null fields",
        axis: Tla,
        anchor: "crates/engine/query/src/plan/type_join/type_join_one.rs check_relation_filter",
        model_ref: "MC_MissingRelationFilter_{Green,Red_NoId,Red_Sibling}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/query/one_to_one/with_filter_secondary_relation_id.rs missing_relation_checks_all_conditions_against_null",
    },
    Property {
        family: "Missing relation filters",
        name: "MissingRelationFilter.without_id_rejected / failing_id_rejected / failing_sibling_rejected / accepted_iff_all_match",
        axis: Lean,
        anchor: "crates/engine/query/src/plan/type_join/type_join_one.rs check_relation_filter",
        model_ref: "MissingRelationFilter (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/query/one_to_one/with_filter_secondary_relation_id.rs missing_relation_checks_all_conditions_against_null",
    },
];
