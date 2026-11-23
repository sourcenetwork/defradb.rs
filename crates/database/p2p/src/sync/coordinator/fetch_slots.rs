//! What holds each pending-DAG fetch slot, for sync status (#2571).

use super::{PendingDagFetchTask, SyncShutdownHandle};

/// Occupancy of the pending-DAG fetch slots `--p2p-max-concurrent-dag-fetches`
/// bounds. Only live entries count: a finished owner's entry is left out even
/// before the registry retires it.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct PendingDagFetchSlots {
    pub limit: usize,
    /// Slots held, in any state; `limit - held` are free.
    pub held: usize,
    /// Reserved by the retry clock and not yet claimed by a fetch or a merge.
    pub scheduled: usize,
    /// Claimed by a fetch task that is still being spawned.
    pub claimed: usize,
    pub running: usize,
    pub merging: usize,
    /// The root holding each slot and its state.
    pub roots: Vec<PendingDagFetchSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PendingDagFetchSlot {
    pub root_cid: String,
    pub state: &'static str,
}

impl SyncShutdownHandle {
    pub(crate) fn pending_dag_fetch_slots(&self) -> PendingDagFetchSlots {
        let mut slots = PendingDagFetchSlots {
            limit: self.inner.pending_dag_fetch_task_limit,
            ..Default::default()
        };
        self.inner.pending_dag_fetch_tasks.peek(|tasks| {
            for (root_cid, task) in tasks.iter().filter(|(_, task)| task.is_live()) {
                let (count, state) = match task {
                    PendingDagFetchTask::Scheduled => (&mut slots.scheduled, "scheduled"),
                    PendingDagFetchTask::Claimed(_) => (&mut slots.claimed, "claimed"),
                    PendingDagFetchTask::Running(_) => (&mut slots.running, "running"),
                    PendingDagFetchTask::Merging(_) => (&mut slots.merging, "merging"),
                };
                *count += 1;
                slots.roots.push(PendingDagFetchSlot {
                    root_cid: root_cid.to_string(),
                    state,
                });
            }
        });
        slots.held = slots.roots.len();
        slots.roots.sort_by(|a, b| a.root_cid.cmp(&b.root_cid));
        slots
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/fetch_slots.rs"]
mod tests;
