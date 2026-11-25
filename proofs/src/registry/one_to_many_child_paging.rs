use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query; a child limit/offset applies per parent so a later parent keeps its page; `crates/engine/query/src/plan/type_join/type_join_many/children.rs`";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "One-to-many child paging",
        name: "LIVE_EveryParentCompletesOrderedPage / INV_PageIsOrderedPrefix / INV_PageWithinLimit / INV_SecondParentKeepsPage: each parent completes its exact ordered child page, not one cut short by a global scan bound",
        axis: Tla,
        anchor: "crates/engine/query/src/plan/type_join/type_join_many/children.rs TypeJoinMany::next_per_parent; crates/engine/query/src/plan/type_join/type_join_many/node.rs TypeJoinMany::with_limit, with_offset",
        model_ref: "MC_ChildPagePerParent_{Green,Green_ShortParent,Red_GlobalBound,Red_NoEarlyStop}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/query/one_to_many/with_limit.rs query_one_to_many_with_child_limit_keeps_a_two_parent_page",
    },
];
