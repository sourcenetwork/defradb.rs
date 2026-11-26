use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "edge";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A network task's completion says how the work ended: its own
        // handle's abort is Cancelled, the runtime's stop is Closed, a value
        // is a value. The handle records its abort before it cancels, and the
        // runtime marks itself stopped before it ends its tasks.
        family: "Edge completion",
        name: "INV_AbortSaysCancelled / INV_StopSaysClosed / INV_DoneSaysValue: a completion its handle aborted says Cancelled, one the runtime's stop cut off says Closed, one that finished says its value",
        axis: Tla,
        anchor: "crates/foundation/edge/src/task.rs Task::poll, Task::abort; crates/foundation/edge/src/platform/native.rs the shutdown; crates/foundation/edge/src/shared.rs Shared::stop",
        model_ref: "MC_EdgeCompletion_{Green,Red_NoMap,Red_CancelBeforeFlag,Red_CancelBeforeStopMark}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/foundation/edge/src/tests/task_set.rs aborting_a_set_stops_its_tasks_and_hands_each_back_cancelled; shutdown.rs shutdown_stops_accepting_drains_then_stops_the_rest; spawn.rs a_spawned_task_hands_back_its_value",
    },
];
