use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // One-to-many child paging (#3051): the scan stays unbounded and each
        // parent stops only when the selected index guarantees the child order.
        family: "Per-parent child paging",
        name: "INV_CompletedPage / INV_EarlyStopOnlyWhenOrdered / LIVE_AllPagesComplete",
        axis: Tla,
        anchor: "crates/engine/query/src/planner/joins/child_plan.rs build_relation_child_plan; crates/engine/query/src/plan/type_join/type_join_many/children.rs TypeJoinMany::next_per_parent",
        model_ref: "MC_ChildLimitPaging_{Green,Ordered,Red_UnorderedStop}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Per-parent child paging",
        name: "stop_after_parent_page",
        axis: Lean,
        anchor: "crates/engine/query/src/plan/type_join/type_join_many/children.rs TypeJoinMany::next_per_parent",
        model_ref: "ChildPage.lean",
        tiers: &[Boundary],
        bound_by: "",
    },
];
