use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Query fragment nesting bound",
        name: "reach_le / walk_none_iff — the fragment walk never calls itself deeper than max_fragment_nesting, and refuses exactly the queries that nest deeper",
        axis: Lean,
        anchor: "crates/engine/query/src/query_parse/limits.rs FragmentPath::check_nesting; crates/engine/query/src/subscription.rs collect_root_fields",
        model_ref: "FragmentNesting.reach_le / walk_none_iff / off_by_one_accepts_too_deep",
        tiers: &[Contract],
        bound_by: "crates/engine/query/tests/fragment_stack.rs a_long_fragment_chain_is_refused_on_a_small_stack, a_chain_at_the_default_limit_still_parses",
    },
];
