use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Relation filters under document access",
        name: "gated_matches_selection / ungated_filter_plan_mismatch / gated_ignores_unreadable / ungated_reveals_unreadable / all_readable_unchanged — a relation filter keeps a parent only by a related document the reader is shown: a selection whose child scan and filter plan are both gated keeps exactly those parents and the gated filter-only join keeps the same ones, a selection whose filter plan is not gated does not, the gated answer does not depend on a related document the reader may not read, and nothing changes for a reader who may read every related document",
        axis: Lean,
        anchor: "crates/engine/query/src/planner/joins/filter_only.rs Planner::apply_filter_only_joins; crates/engine/query/src/planner/joins/secondary_id.rs Planner::apply_secondary_id_joins; crates/engine/query/src/planner/joins/child_plan.rs Planner::build_relation_child_plan; crates/engine/query/src/planner/builder/mod.rs Planner::maybe_wrap_with_acp_filter; crates/engine/query/src/runner/mutation_inputs.rs QueryRunner::resolve_filter_to_doc_ids",
        model_ref: "RelationFilterAccess.gated_matches_selection / ungated_filter_plan_mismatch / gated_ignores_unreadable / ungated_reveals_unreadable / all_readable_unchanged",
        tiers: &[Contract],
        bound_by: "tools/integration-test/tests/testo/acp/dac/relation_filters.rs a_many_to_one_filter_matches_only_readable_related_documents, a_one_to_many_filter_matches_only_readable_related_documents, a_one_to_one_filter_matches_only_readable_related_documents, a_two_hop_filter_matches_only_readable_related_documents, an_indexed_relation_filter_matches_only_readable_related_documents, an_indexed_one_to_many_filter_matches_only_readable_related_documents, a_mutation_through_a_relation_filter_picks_only_readable_related_documents, a_secondary_relation_id_names_only_a_readable_related_document, a_secondary_relation_id_filter_sees_an_unreadable_related_document_as_null",
    },
];
