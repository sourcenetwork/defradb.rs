use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db-index (vector), schema, query";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Deferred vector maintenance",
        name: "INV_SearchSeesLatest — a search on a deferred vector index returns each document's latest committed vector and no deleted document, whatever the drain has applied",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/index.rs search_where; crates/database/db/src/index/vector/index/deferred.rs defers_now, supersede_pending, pending_entries; crates/database/db/src/index/vector/pending/search.rs merge_pending",
        model_ref: "MC_DeferredVector_Green.cfg (GREEN) / MC_DeferredVector_Red_NoMerge.cfg / MC_DeferredVector_Red_KeepStale.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/index/deferred_vector.rs a_deferred_write_is_found_before_the_drain_applies_it, an_update_and_a_delete_before_the_drain_hide_the_stale_graph",
    },
    Property {
        family: "Deferred vector maintenance",
        name: "INV_PendingIsLatest / INV_GraphSettled — a pending entry is always its node's latest write, and a node with nothing pending has the graph agree with its document: the drain deletes only entries it applied, and an inline write supersedes an older entry",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/pending/drain.rs drain_vector_turn, apply_attempt; crates/database/db/src/index/vector/pending/mod.rs get_pending, put_pending, delete_pending; crates/database/db/src/index/vector/index/deferred.rs apply_planned, apply_all_pending, supersede_pending; crates/database/db/src/index/vector/index.rs rebuild_if_due",
        model_ref: "MC_DeferredVector_Green.cfg (GREEN) / MC_DeferredVector_Red_NoValidate.cfg / MC_DeferredVector_Red_NoSupersede.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/index/deferred_vector.rs a_write_past_the_pending_limit_updates_the_graph_itself, concurrent_deferred_writers_do_not_conflict, a_rebuild_applies_the_pending_writes_rather_than_clearing_them",
    },
    Property {
        family: "Deferred vector maintenance",
        name: "PROP_Converges — once writes stop, nothing stays pending and the graph holds every latest vector, across a crash: each deferred commit asks for a drain and a node that opens drains what is pending",
        axis: Tla,
        anchor: "crates/database/db/src/index/manager/deferred.rs IndexManager::ask_vector_drains; crates/database/db/src/index/deferred/task.rs drain_turn, deferred_indexes; crates/database/db/src/index/vector/pending/drain.rs drain_vector_turn",
        model_ref: "MC_DeferredVector_Green.cfg (GREEN) / MC_DeferredVector_Red_NoAsk.cfg / MC_DeferredVector_Red_NoSettle.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/index/deferred_vector.rs pending_writes_survive_a_restart_and_drain_after_it",
    },
    Property {
        family: "Deferred vector maintenance",
        name: "decode_encode / decode_empty / decode_partial_element / decode_unknown_op — a pending entry reads back as exactly its operation and vector, and no malformed value reads as an entry",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/pending/entry.rs encode_value, decode_value",
        model_ref: "VectorPending.decode_encode / decode_empty / decode_partial_element / decode_unknown_op",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/pending_entry.rs an_entry_reads_back_as_written, the_value_is_gos_bytes, a_malformed_value_is_refused_with_gos_text",
    },
];
