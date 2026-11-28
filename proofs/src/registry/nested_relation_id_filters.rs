use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Nested relation ID filters",
        name: "NestedRelationIdFilter.scan_never_drops_a_match / answer_is_full_filter / pruning_an_or_branch_loses_a_match",
        axis: Lean,
        anchor: "crates/engine/query/src/mapper/filter/secondary_id.rs without_secondary_relation_ids; crates/engine/query/src/planner/builder/filter_prep.rs prepare_filter",
        model_ref: "NestedRelationIdFilter (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/query/one_to_one/with_filter_nested_secondary_relation_id.rs or_with_secondary_relation_id_and_sibling_matches_either",
    },
];
