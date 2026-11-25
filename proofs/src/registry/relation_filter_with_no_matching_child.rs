use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query (#3087)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Relation filter with no matching child",
        name: "empty_cache_keeps_nothing / shortcut_same_rows / orphans_need_the_walk — a primary relation-filter join whose child cache is empty returns the rows of the full parent walk without reading a parent, and an exhaustive join, which also returns orphans, must not take that short cut",
        axis: Lean,
        anchor: "crates/engine/query/src/plan/type_join/type_join_one.rs TypeJoinOne::no_child_can_pass, TypeJoinOne::init, TypeJoinOne::next",
        model_ref: "RelationFilterNoChild.empty_cache_keeps_nothing / shortcut_same_rows / orphans_need_the_walk",
        tiers: &[Contract],
        bound_by: "crates/engine/query/src/planner/joins/relation_filter_execution_tests.rs no_matching_child_reads_no_parent, no_matching_child_reads_no_parent_of_an_index_ordered_scan",
    },
];
