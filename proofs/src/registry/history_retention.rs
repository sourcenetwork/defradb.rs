use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, query";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Pruning a collection's history to @retention(maxDepth: N): one
        // transact attempt per document keeps the top N heights, every live
        // composite and field head and every block another document owns
        // (a validated owner scan decides each delete), reclaims every other
        // field block the priority index names, and keeps the pruned commits'
        // priority entries so a replayed pruned commit stays applied (and is
        // taken again by the next prune) and a version read that meets a
        // pruned block fails instead of returning a partial document.
        family: "History retention",
        name: "INV_HeadsStored / INV_FieldHeadsStored / INV_OwnedStored / INV_WindowKept / INV_NoStaleHead / INV_Pruned / INV_Reclaimed / INV_KeptLinksStored / INV_NoPartialRead",
        axis: Tla,
        anchor: "crates/database/db/src/collection/ops/prune_history.rs DB::prune_history_as, DB::prune_doc_history, prune_doc_in; crates/database/db/src/block/cleanup.rs delete_owned_commit, delete_owned_block; crates/database/db/src/docid/map.rs block_has_owners; crates/database/db/src/merge/merge_handler/history.rs composite_was_applied; crates/database/db/src/read/versioned.rs VersionedFetcher::collect_blocks_to_genesis, VersionedFetcher::refuse_pruned; crates/engine/query/src/runner/version.rs QueryRunner::execute_cid_query_with_version; crates/engine/query/src/error.rs QueryError::HistoryPruned",
        model_ref: "MC_HistoryRetention_{Green,Red_PruneHead,Red_PruneFieldHead,Red_PlainScan,Red_DropMarker,Red_OffByOne,Red_LinksOnly,Red_DropLinks,Red_SkipRead}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "History retention",
        name: "head_kept / window_kept / short_history_kept / below_pruned / stale_top_safe / linked_kept / field_head_kept / shared_kept / unkept_reclaimed / superseded_head_reclaimed",
        axis: Lean,
        anchor: "crates/database/db/src/collection/ops/prune_history.rs prune_doc_in; crates/database/db/src/block/cleanup.rs delete_owned_block",
        model_ref: "HistoryWindow (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
