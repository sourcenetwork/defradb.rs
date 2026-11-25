use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The orphan block sweep: a fetched block's to-merge mark
        // holds its fetch time and a refetch leaves it alone; the sweep nominates
        // expired marks from a snapshot and decides each in a transaction
        // that reads the mark and the owners again, deleting an unowned
        // block, clearing the mark of an owned one, and never touching an
        // untimed mark; a merge clears and owns its signature block too.
        family: "Orphan block sweep",
        name: "INV_NoMergedLoss / INV_NoOwnedLoss / INV_MergedUnmarked / INV_SigOwned / INV_LegacyKept / PROP_Reclaims",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/blockstore.rs BlockWritesAttempt::run, marker_value, marker_time; crates/database/db/src/block/orphan_gc.rs DB::reclaim_orphan_blocks, DB::decide_orphans; crates/database/db/src/merge/merge_handler/doc_identity.rs record_block_ownership",
        model_ref: "MC_OrphanGc_{Green,Red_SnapshotRead,Red_NoOwnerCheck,Red_BlindRefresh,Red_ReclaimLegacy,Red_SigMarked,Red_SigUnowned}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The sweep does not read the pending-DAG store, so it reclaims the
        // fetched blocks of a durable obligation stalled past the TTL. The
        // model's weak fairness on Fetch assumes such a block can be fetched
        // again.
        family: "Orphan block sweep",
        name: "PROP_Reclaims assumes a reclaimed unmerged block can be fetched again",
        axis: Tla,
        anchor: "crates/database/db/src/block/orphan_gc.rs DB::decide_orphans, OrphanSweepConfig::TTL",
        model_ref: "MC_OrphanGc_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Orphan block sweep",
        name: "time_value / time_short / time_long / time_legacy",
        axis: Lean,
        anchor: "crates/engine/storage/src/stores/blockstore.rs marker_value, marker_time",
        model_ref: "OrphanMarker (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
