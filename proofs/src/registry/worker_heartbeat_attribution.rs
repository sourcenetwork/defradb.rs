use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "workers";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A task poll hands the heartbeat to the hook poll after it in one
        // store; the hook stays shown until the next task poll or the turn's
        // end, and the monitor times it from its first sighting. The stuck
        // mark names the poll it is for, so a slow hook poll is never
        // reported or remembered as the task's. Two monitors scan at once:
        // neither replaces the mark of the poll shown now, and a task or hook
        // poll is reported only by a claim that raises the last claimed word.
        family: "Worker heartbeat attribution",
        name: "INV_ReportTrue / INV_MarkTrue / INV_NotLeftStuck / INV_KeepsMark / INV_TaskOnce / INV_HookOnce: the monitors report only a poll that is still the worker's and past the limit; a task is remembered as reported only for a long poll of its own; no mark outlives its poll and the worker's look; a poll marked while shown keeps its mark while shown; no task or hook poll is reported twice",
        axis: Tla,
        anchor: "crates/foundation/workers/src/worker/local.rs Local::poll, Local::after_poll; crates/foundation/workers/src/worker/hook_beat.rs Local::begin_hook, Local::close_hook, Local::end_beat; crates/foundation/workers/src/worker/run.rs HookState::poll, HookState::poll_begun; crates/foundation/workers/src/worker/heartbeat.rs Heartbeat::begin_hook; crates/foundation/workers/src/worker/shared.rs WorkerShared::mark_stuck, WorkerShared::unmark, WorkerShared::sweep_mark, WorkerShared::hook_elapsed, WorkerShared::claim_task_report, WorkerShared::claim_hook_report; crates/foundation/workers/src/pool/monitor.rs scan",
        model_ref: "MC_WorkerHeartbeat_{Green,Red_Main,Red_NoHookBeat,Red_FlagMark,Red_NoHookClear,Red_NoRecheck,Red_MarkOverwrite,Red_SwapClaim,Red_TaskSwapClaim}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
