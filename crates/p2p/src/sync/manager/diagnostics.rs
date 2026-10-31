//! Counters for P2P sync observability.
//!
//! Increments are cheap atomic operations; snapshots provide a point-in-time
//! view so integration tests can assert bounded retry behavior without
//! scraping noisy logs (see issue #858).
//!
//! Every counter is scoped to one `SyncManager`. Counters the transport keeps
//! for its node (gossip decode failures, the Shinzo wire) live in
//! [`WireStats`](crate::sync::WireStats).

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct SyncDiagnostics {
    car_empty_responses: AtomicU64,
    car_no_blocks_served: AtomicU64,
    car_requested_cids: AtomicU64,
    car_present_cids: AtomicU64,
    car_served_cids: AtomicU64,
    car_filtered_cids: AtomicU64,
    provider_rotations: AtomicU64,
    missing_link_retries: AtomicU64,
    pending_dag_resolved: AtomicU64,
    pending_dag_registered: AtomicU64,
    pending_dag_expired: AtomicU64,
    pending_dag_high_water: AtomicU64,
    persisted_pending_dag_high_water: AtomicU64,
    single_flight_suppressed: AtomicU64,
    already_merged_fast_path: AtomicU64,
    pending_dag_capacity_shed: AtomicU64,
    pending_dag_retry_dispatched: AtomicU64,
    pending_dag_retry_suppressed: AtomicU64,
    pending_dag_fetch_deferred_unavailable: AtomicU64,
    pending_dag_fetch_deferred_contention: AtomicU64,
    pending_dag_fetch_exhausted: AtomicU64,
    pending_dag_terminal_merged: AtomicU64,
    pending_dag_terminal_quarantined: AtomicU64,
    arrivals: super::arrival_stats::ArrivalStats,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncDiagnosticsSnapshot {
    pub car_empty_responses: u64,
    pub car_no_blocks_served: u64,
    pub car_requested_cids: u64,
    pub car_present_cids: u64,
    pub car_served_cids: u64,
    pub car_filtered_cids: u64,
    pub provider_rotations: u64,
    pub missing_link_retries: u64,
    pub pending_dag_resolved: u64,
    pub pending_dag_registered: u64,
    pub pending_dag_expired: u64,
    pub pending_dag_high_water: u64,
    pub persisted_pending_dag_high_water: u64,
    pub single_flight_suppressed: u64,
    pub already_merged_fast_path: u64,
    pub pending_dag_capacity_shed: u64,
    pub pending_dag_retry_dispatched: u64,
    pub pending_dag_retry_suppressed: u64,
    pub pending_dag_fetch_deferred_unavailable: u64,
    pub pending_dag_fetch_deferred_contention: u64,
    pub pending_dag_fetch_exhausted: u64,
    pub pending_dag_terminal_merged: u64,
    /// Pending-DAG registrations moved to quarantine after a deterministic
    /// merge rejection (#1128). See `SyncManager::quarantine_pending_dag`.
    pub pending_dag_terminal_quarantined: u64,
}

impl SyncDiagnostics {
    /// Arrival outcomes since the last `p2p stats` report.
    pub fn arrivals(&self) -> &super::arrival_stats::ArrivalStats {
        &self.arrivals
    }

    pub fn snapshot(&self) -> SyncDiagnosticsSnapshot {
        SyncDiagnosticsSnapshot {
            car_empty_responses: self.car_empty_responses.load(Ordering::Relaxed),
            car_no_blocks_served: self.car_no_blocks_served.load(Ordering::Relaxed),
            car_requested_cids: self.car_requested_cids.load(Ordering::Relaxed),
            car_present_cids: self.car_present_cids.load(Ordering::Relaxed),
            car_served_cids: self.car_served_cids.load(Ordering::Relaxed),
            car_filtered_cids: self.car_filtered_cids.load(Ordering::Relaxed),
            provider_rotations: self.provider_rotations.load(Ordering::Relaxed),
            missing_link_retries: self.missing_link_retries.load(Ordering::Relaxed),
            pending_dag_resolved: self.pending_dag_resolved.load(Ordering::Relaxed),
            pending_dag_registered: self.pending_dag_registered.load(Ordering::Relaxed),
            pending_dag_expired: self.pending_dag_expired.load(Ordering::Relaxed),
            pending_dag_high_water: self.pending_dag_high_water.load(Ordering::Relaxed),
            persisted_pending_dag_high_water: self
                .persisted_pending_dag_high_water
                .load(Ordering::Relaxed),
            single_flight_suppressed: self.single_flight_suppressed.load(Ordering::Relaxed),
            already_merged_fast_path: self.already_merged_fast_path.load(Ordering::Relaxed),
            pending_dag_capacity_shed: self.pending_dag_capacity_shed.load(Ordering::Relaxed),
            pending_dag_retry_dispatched: self.pending_dag_retry_dispatched.load(Ordering::Relaxed),
            pending_dag_retry_suppressed: self.pending_dag_retry_suppressed.load(Ordering::Relaxed),
            pending_dag_fetch_deferred_unavailable: self
                .pending_dag_fetch_deferred_unavailable
                .load(Ordering::Relaxed),
            pending_dag_fetch_deferred_contention: self
                .pending_dag_fetch_deferred_contention
                .load(Ordering::Relaxed),
            pending_dag_fetch_exhausted: self.pending_dag_fetch_exhausted.load(Ordering::Relaxed),
            pending_dag_terminal_merged: self.pending_dag_terminal_merged.load(Ordering::Relaxed),
            pending_dag_terminal_quarantined: self
                .pending_dag_terminal_quarantined
                .load(Ordering::Relaxed),
        }
    }

    pub fn record_car_empty_response(&self) {
        self.car_empty_responses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_car_no_blocks_served(&self) {
        self.car_no_blocks_served.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_car_serve_counts(
        &self,
        requested: usize,
        present: usize,
        served: usize,
        filtered: usize,
    ) {
        self.car_requested_cids
            .fetch_add(requested as u64, Ordering::Relaxed);
        self.car_present_cids
            .fetch_add(present as u64, Ordering::Relaxed);
        self.car_served_cids
            .fetch_add(served as u64, Ordering::Relaxed);
        self.car_filtered_cids
            .fetch_add(filtered as u64, Ordering::Relaxed);
    }

    pub fn record_provider_rotation(&self) {
        self.provider_rotations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_missing_link_retry(&self) {
        self.missing_link_retries.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_resolved(&self) {
        self.pending_dag_resolved.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_registered(&self) {
        self.pending_dag_registered.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_expired(&self) {
        self.pending_dag_expired.fetch_add(1, Ordering::Relaxed);
    }

    pub fn observe_pending_dag_depth(&self, current: usize) {
        self.pending_dag_high_water
            .fetch_max(current as u64, Ordering::Relaxed);
    }

    pub fn observe_persisted_pending_dag_depth(&self, current: usize) {
        self.persisted_pending_dag_high_water
            .fetch_max(current as u64, Ordering::Relaxed);
    }

    pub fn record_single_flight_suppressed(&self) {
        self.single_flight_suppressed
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_already_merged_fast_path(&self) {
        self.already_merged_fast_path
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_capacity_shed(&self) {
        self.pending_dag_capacity_shed
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_retry_dispatched(&self) {
        self.pending_dag_retry_dispatched
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_retry_suppressed(&self) {
        self.pending_dag_retry_suppressed
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_fetch_deferred_unavailable(&self) {
        self.pending_dag_fetch_deferred_unavailable
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_fetch_deferred_contention(&self) {
        self.pending_dag_fetch_deferred_contention
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_fetch_exhausted(&self) {
        self.pending_dag_fetch_exhausted
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_terminal_merged(&self) {
        self.pending_dag_terminal_merged
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_pending_dag_terminal_quarantined(&self) {
        self.pending_dag_terminal_quarantined
            .fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_manager_counters_start_at_zero() {
        let diag = SyncDiagnostics::default();
        let snap = diag.snapshot();
        assert_eq!(snap.car_empty_responses, 0);
        assert_eq!(snap.car_no_blocks_served, 0);
        assert_eq!(snap.provider_rotations, 0);
        assert_eq!(snap.missing_link_retries, 0);
        assert_eq!(snap.pending_dag_resolved, 0);
        assert_eq!(snap.pending_dag_registered, 0);
        assert_eq!(snap.pending_dag_expired, 0);
        assert_eq!(snap.pending_dag_high_water, 0);
        assert_eq!(snap.persisted_pending_dag_high_water, 0);
        assert_eq!(snap.single_flight_suppressed, 0);
        assert_eq!(snap.already_merged_fast_path, 0);
        assert_eq!(snap.pending_dag_capacity_shed, 0);
        assert_eq!(snap.pending_dag_retry_dispatched, 0);
        assert_eq!(snap.pending_dag_retry_suppressed, 0);
        assert_eq!(snap.pending_dag_fetch_deferred_unavailable, 0);
        assert_eq!(snap.pending_dag_fetch_deferred_contention, 0);
        assert_eq!(snap.pending_dag_fetch_exhausted, 0);
        assert_eq!(snap.pending_dag_terminal_merged, 0);
        assert_eq!(snap.pending_dag_terminal_quarantined, 0);
    }

    #[test]
    fn each_per_manager_record_increments_its_counter() {
        let diag = SyncDiagnostics::default();
        diag.record_car_empty_response();
        diag.record_car_empty_response();
        diag.record_car_no_blocks_served();
        diag.record_provider_rotation();
        diag.record_missing_link_retry();
        diag.record_pending_dag_resolved();
        diag.record_pending_dag_registered();
        diag.record_pending_dag_expired();
        diag.observe_pending_dag_depth(2);
        diag.observe_pending_dag_depth(1);
        diag.observe_persisted_pending_dag_depth(3);
        diag.record_single_flight_suppressed();
        diag.record_already_merged_fast_path();
        diag.record_pending_dag_capacity_shed();
        diag.record_pending_dag_retry_dispatched();
        diag.record_pending_dag_retry_suppressed();
        diag.record_pending_dag_fetch_deferred_unavailable();
        diag.record_pending_dag_fetch_deferred_contention();
        diag.record_pending_dag_fetch_exhausted();
        diag.record_pending_dag_terminal_merged();
        diag.record_pending_dag_terminal_quarantined();

        let snap = diag.snapshot();
        assert_eq!(snap.car_empty_responses, 2);
        assert_eq!(snap.car_no_blocks_served, 1);
        assert_eq!(snap.provider_rotations, 1);
        assert_eq!(snap.missing_link_retries, 1);
        assert_eq!(snap.pending_dag_resolved, 1);
        assert_eq!(snap.pending_dag_registered, 1);
        assert_eq!(snap.pending_dag_expired, 1);
        assert_eq!(snap.pending_dag_high_water, 2);
        assert_eq!(snap.persisted_pending_dag_high_water, 3);
        assert_eq!(snap.single_flight_suppressed, 1);
        assert_eq!(snap.already_merged_fast_path, 1);
        assert_eq!(snap.pending_dag_capacity_shed, 1);
        assert_eq!(snap.pending_dag_retry_dispatched, 1);
        assert_eq!(snap.pending_dag_retry_suppressed, 1);
        assert_eq!(snap.pending_dag_fetch_deferred_unavailable, 1);
        assert_eq!(snap.pending_dag_fetch_deferred_contention, 1);
        assert_eq!(snap.pending_dag_fetch_exhausted, 1);
        assert_eq!(snap.pending_dag_terminal_merged, 1);
        assert_eq!(snap.pending_dag_terminal_quarantined, 1);
    }

    #[test]
    fn provider_rotations_are_manager_local() {
        let first = SyncDiagnostics::default();
        let second = SyncDiagnostics::default();
        first.record_provider_rotation();

        assert_eq!(first.snapshot().provider_rotations, 1);
        assert_eq!(second.snapshot().provider_rotations, 0);
    }

    #[test]
    fn concurrent_increments_are_not_lost() {
        use std::sync::Arc;
        use std::thread;

        let diag = Arc::new(SyncDiagnostics::default());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let d = Arc::clone(&diag);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    d.record_car_empty_response();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(diag.snapshot().car_empty_responses, 8_000);
    }
}
