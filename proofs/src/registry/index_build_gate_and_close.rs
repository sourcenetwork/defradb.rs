use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Closing the database while an index build waits at its build
        // gate: the wait also ends on the close, so the pool, which waits
        // for the build task, stops; the stopped build keeps its record in
        // progress, as after a crash; the next open returns without waiting
        // for it, and the node then resumes it as a task of its own, so the
        // build finishes once the gate opens.
        family: "Index build gate and close",
        name: "INV_BuiltOnlyWhenIndexed / LIVE_CloseFinishes / LIVE_BuildFinishes",
        axis: Tla,
        anchor: "crates/database/db/src/index/backfill.rs DB::backfill_batches, DB::backfill_index, DB::start_resumed_index_backfills, DB::resume_index_backfills; crates/database/db/src/database/mod.rs DB::close, DB::closing, DB::finish_open; crates/entry-points/defra-node/src/assemble.rs assemble; crates/database/db/src/index/build_gate.rs IndexBuildGate::enter",
        model_ref: "MC_IndexBuildGateClose_{Green,Red_NoRace,Red_FailOnClose,Red_Forget,Red_OpenAwaits}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
