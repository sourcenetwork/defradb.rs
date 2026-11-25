use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query (#3276)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Pure functions over a join's filter, page and rendered fields; the Rust
        // side is exercised by the testo scenarios tagged
        // defra.query.one_to_many.aggregate.beside-paged-selection.
        family: "Aggregate join reuse",
        name: "reuse_reads_own_rows / selection_share_reads_own_rows / reuse_keeps_read_fields — an aggregate reads another join's rows only when they are exactly the rows and fields its own scan would give",
        axis: Lean,
        anchor: "crates/engine/query/src/planner/joins/aggregate_join_reuse.rs AggregateJoin::serves, scan_filter; crates/engine/query/src/planner/joins/aggregate_joins.rs Planner::apply_aggregate_joins; crates/engine/query/src/planner/joins/selection.rs build_selection_join_info; crates/engine/query/src/planner/joins/child_plan.rs Planner::build_relation_child_plan",
        model_ref: "AggregateJoinReuse.reuse_reads_own_rows / selection_share_reads_own_rows / reuse_keeps_read_fields / limited_selection_loses_rows / filtered_selection_hides_rows_from_unfiltered / relation_only_reuse_reads_other_rows / unrendered_field_is_missing",
        tiers: &[Contract],
        bound_by: "tools/integration-test/tests/testo/query/one_to_many/with_limited_selection_aggregates.rs",
    },
];
