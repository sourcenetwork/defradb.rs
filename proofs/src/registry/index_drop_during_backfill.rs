use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db-index";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Dropping an index while its backfill runs: the drop's commit takes
        // the backfill record with it, a backfill failure is recorded only
        // while the index is defined, entries go in pages after the
        // definition commits, the drop retires the index's id so a new
        // index never shares its keys, and a restart finishes an unfinished
        // drop.
        family: "Index drop during backfill",
        name: "INV_NoRecordForGoneIndex / INV_NoOrphanEntries / INV_NewIndexIntact / LIVE_DropFinishes",
        axis: Tla,
        anchor: "crates/database/db/src/index/drop.rs DB::drop_index, DropIndexAttempt, DropPage, DB::remove_dropped_entries, DB::resume_index_drops; crates/database/db/src/index/manager/mod.rs index_id_floor; crates/database/db/src/index/backfill.rs BackfillPage::index, DB::backfill_index, DB::resume_index_backfills; crates/database/db/src/index/settle.rs FailBackfillAttempt",
        model_ref: "MC_IndexDropBackfill_{Green,Red_KeepRecord,Red_BlindFail,Red_SameTxn,Red_Forget,Red_ReuseId}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
