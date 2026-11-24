use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p, edge";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The first shutdown caller to swap the flag builds the one drain,
        // which owns the only completion sender; a closed edge hands the
        // drain back and the winner runs it in place. The drain waits for
        // pruners, takes the tracked set, and aborts what outlives its
        // deadline, so no tracked task keeps the coordinator alive. The
        // GREEN configurations assume prune_finished_tasks reads the flag
        // after announcing itself; Red_PrunerIgnoresFlag is the code without
        // that read.
        family: "Coordinator shutdown drain",
        name: "INV_NoLeak / INV_OneDrain / INV_ErrMeansCancelled / LIVE_CallersReturn: one drain runs, no tracked task is left running in the set once it returns, every shutdown caller returns",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs SyncShutdownHandle::shutdown, begin_shutdown, drain_background_tasks, spawn_task, spawn_here, register_task, prune_finished_tasks; crates/database/p2p/src/network.rs Network::try_spawn; crates/foundation/edge/src/handle.rs Edge::try_spawn",
        model_ref: "MC_CoordinatorShutdown_{Green,Green_TwoSpawns,Red_DropRefused,Red_CheckThenSpawn,Red_NoAbort,Red_SenderOutsideDrain,Red_TwoWinners,Red_NoPrunerWait,Red_PrunerIgnoresFlag,Red_NoRecheck}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/tests/coordinator/shutdown.rs concurrent_shutdown_callers_wait_for_task_completion, shutdown_rejects_new_tasks_without_spawning_them, task_registration_racing_shutdown_does_not_leave_resources_alive, shutdown_on_a_closed_edge_runs_the_drain_and_releases_tracked_tasks",
    },
];
