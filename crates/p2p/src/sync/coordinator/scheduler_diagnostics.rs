//! Aggregate, bounded observations of the pending-DAG scheduler.

use blockstore::Blockstore;
use serde::Serialize;

use super::{PendingDagFetchTask, SyncCoordinator};
use crate::transport::P2PTransport;

const PEER_SAMPLE_LIMIT: usize = 256;

/// Independently sampled gauges; concurrent changes can make them disagree.
/// No root, collection, document or peer identifiers are included.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SchedulerDiagnostics {
    pub due_roots: usize,
    pub eligible_roots: usize,
    pub storage_blocked_roots: usize,
    pub storage_blocked_active_roots: usize,
    pub unregistered_roots: usize,
    pub reservations_capacity: usize,
    pub reservations_scheduled: usize,
    pub reservations_claimed: usize,
    pub reservations_running: usize,
    pub reservations_merging: usize,
    /// Includes pushes between reservation and queue insertion.
    pub dispatch_lane_queued_or_entering: usize,
    pub dispatch_lane_capacity: usize,
    pub global_fetch_permits_capacity: usize,
    pub global_fetch_permits_in_use: usize,
    pub peer_permits_per_peer: usize,
    /// Sum over at most `peer_entries_sampled` peers, not necessarily all peers.
    pub peer_permits_in_use_sampled: usize,
    pub peer_entries_sampled: usize,
    pub peer_entries_truncated: bool,
    pub provider_lookup_timed_out_total: u64,
}

impl<B: Blockstore + 'static, T: P2PTransport> SyncCoordinator<B, T> {
    pub fn scheduler_diagnostics(&self) -> SchedulerDiagnostics {
        let (due, eligible, blocked, active_blocker, unregistered) =
            self.manager.pending_scheduler_counts();
        let shutdown = &self.runtime.shutdown.inner;
        let mut snapshot = SchedulerDiagnostics {
            due_roots: due,
            eligible_roots: eligible,
            storage_blocked_roots: blocked,
            storage_blocked_active_roots: active_blocker,
            unregistered_roots: unregistered,
            reservations_capacity: shutdown.pending_dag_fetch_task_limit,
            provider_lookup_timed_out_total: self
                .manager
                .diagnostics()
                .snapshot()
                .provider_lookup_timed_out_total,
            ..SchedulerDiagnostics::default()
        };
        shutdown.pending_dag_fetch_tasks.peek(|tasks| {
            for task in tasks.values().filter(|task| task.is_live()) {
                match task {
                    PendingDagFetchTask::Scheduled => snapshot.reservations_scheduled += 1,
                    PendingDagFetchTask::Claimed => snapshot.reservations_claimed += 1,
                    PendingDagFetchTask::Running(_) => snapshot.reservations_running += 1,
                    PendingDagFetchTask::Merging(_) => snapshot.reservations_merging += 1,
                }
            }
        });
        (
            snapshot.dispatch_lane_queued_or_entering,
            snapshot.dispatch_lane_capacity,
        ) = self.manager.dispatch_lane_occupancy();
        let limiter = &self.runtime.dag_fetch_limiter;
        snapshot.global_fetch_permits_capacity = limiter.global_limit;
        snapshot.global_fetch_permits_in_use = limiter
            .global_limit
            .saturating_sub(limiter.global.available_permits());
        snapshot.peer_permits_per_peer = limiter.peer_limit;
        snapshot.peer_entries_truncated = limiter.per_peer.len() > PEER_SAMPLE_LIMIT;
        for semaphore in limiter.per_peer.values().take(PEER_SAMPLE_LIMIT) {
            snapshot.peer_entries_sampled += 1;
            snapshot.peer_permits_in_use_sampled += limiter
                .peer_limit
                .saturating_sub(semaphore.available_permits());
        }
        snapshot
    }
}
