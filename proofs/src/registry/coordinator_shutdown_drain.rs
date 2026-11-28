use super::{Axis::*, Property, Tier::*};

/// The crates covered by the shutdown checks.
pub(super) const CRATES: &str = "p2p, edge";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Coordinator shutdown drain",
        name: "INV_NoLeak / INV_KeepActive / INV_OneDrain / INV_ErrMeansCancelled / LIVE_CallersReturn: one drain waits for registration, pruning preserves live handles, and every shutdown caller returns",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs SyncShutdownHandle::shutdown, begin_shutdown, drain_background_tasks, spawn_task, spawn_pending_dag_fetch, begin_task_registry_access, TaskRegistryAccess::register_task, prune_finished_tasks, drop; crates/database/p2p/src/network.rs Network::try_spawn; crates/foundation/edge/src/handle.rs Edge::try_spawn",
        model_ref: "MC_CoordinatorShutdown_{Green,Green_TwoSpawns,Red_DropRefused,Red_CheckThenSpawn,Red_NoAbort,Red_SenderOutsideDrain,Red_TwoWinners,Red_NoPrunerWait,Red_PrunerIgnoresFlag,Red_DropActive}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/tests/coordinator/shutdown.rs concurrent_shutdown_callers_wait_for_task_completion, drain_waits_for_an_admitted_registration_to_publish, task_registration_racing_shutdown_does_not_leave_resources_alive, shutdown_on_a_closed_edge_runs_the_drain_and_releases_tracked_tasks; crates/database/p2p/src/sync/coordinator/mod.rs pending_dag_fetches_retire_finished_handles",
    },
    Property {
        family: "Coordinator shutdown drain",
        name: "admitted_announces_before_drain / admitted_access_prevents_zero / last_exit_observes_shutdown: the drain sees admitted registrations and receives the final exit signal",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/mod.rs begin_task_registry_access, drain_background_tasks, TaskRegistryAccess::drop",
        model_ref: "CoordinatorRegistry (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/tests/coordinator/shutdown.rs drain_waits_for_an_admitted_registration_to_publish",
    },
];
