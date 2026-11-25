use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query (#2427, #2430)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Pure functions over the schema; the Rust side is exercised by
        // crates/engine/query/src/planner/joins/relation_paths_tests.rs and the
        // validate_select tests in crates/engine/query/src/runner/plan_validation.rs.
        family: "Filter relation paths and scalar leaves",
        name: "cut_eq_longest / longest_max / cut_idem / relationPaths_of_allRel — a filter path is cut at its first non-relation field, idempotently, and all-relation paths are unchanged",
        axis: Lean,
        anchor: "crates/engine/query/src/planner/joins/relation_paths.rs Planner::relation_filter_paths, Planner::relation_prefix",
        model_ref: "JsonSubPathCut.cut_eq_longest / longest_allRel / longest_max / cut_idem / cut_of_allRel / relationPaths_of_allRel",
        tiers: &[Contract],
        bound_by: "crates/engine/query/src/planner/joins/relation_paths_tests.rs json_sub_path_is_not_a_relation_path, relation_path_stops_at_json_field, json_sub_path_filter_through_relation_plans",
    },
    Property {
        family: "Filter relation paths and scalar leaves",
        name: "newValidate_false_iff / newRun_eq_mainRun — validation rejects exactly a sub-selection on a non-relation field, and the queries that run are main's",
        axis: Lean,
        anchor: "crates/engine/query/src/runner/plan_validation.rs validate_select; crates/engine/query/src/planner/joins/selection_join.rs Planner::apply_selection_relation_join",
        model_ref: "JsonSubPathCut.leafOk_false_iff / newValidate_false_iff / newValidate_of_main / newRun_eq_mainRun",
        tiers: &[Contract],
        bound_by: "crates/engine/query/src/planner/joins/relation_paths_tests.rs sub_selection_on_json_field_is_go_validation_error, json_sub_path_filter_plans_without_index",
    },
];
