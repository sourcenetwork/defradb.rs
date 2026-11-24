use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query (#3042)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Joins on a parent read by key",
        name: "lookup_same_rows / lookup_keeps_only_passing / raw_attach_keeps_a_rejected_parent / lookup_many_same_rows / lookup_many_keeps_only_passing / pass_through_drops_the_children — a parent an index-driven join reads by key is joined by the plan that join wraps, a list relation with all its children, so the rows are those of the full walk and a parent whose other relation fails its filter is not returned",
        axis: Lean,
        anchor: "crates/engine/query/src/plan/type_join/type_join_one.rs TypeJoinOne::join_fetched, TypeJoinOne::child_of_fetched, TypeJoinOne::next_inverted_index, TypeJoinOne::next_ordered_primary_inner; crates/engine/query/src/plan/type_join/type_join_many/plan_node.rs TypeJoinMany::join_fetched; crates/engine/query/src/plan/type_join/type_join_many/fetched.rs TypeJoinMany::join_children_of_fetched",
        model_ref: "JoinFetchedParent.lookup_same_rows / lookup_keeps_only_passing / raw_attach_keeps_a_rejected_parent / lookup_many_same_rows / lookup_many_keeps_only_passing / pass_through_drops_the_children",
        tiers: &[Contract],
        bound_by: "tools/integration-test/tests/testo/query/one_to_many/with_relation_filter_indexed_fk.rs relation_filter_through_an_indexed_fk_keeps_a_nested_joined_relation, relation_filter_through_an_indexed_fk_keeps_a_second_relation_filter, relation_filter_through_an_indexed_fk_keeps_a_joined_list_relation, relation_filter_through_an_indexed_fk_keeps_a_list_relation_filter",
    },
];
