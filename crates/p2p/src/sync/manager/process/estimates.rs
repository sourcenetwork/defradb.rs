//! The pending map's estimates, measured on every dispatch pass and exported
//! through the sync status. Nothing here changes what a pass dispatches.

use cid::Cid;

use blockstore::Blockstore;

use crate::sync::arbitration::{tie_break_reorders, PendingPeerOccupancy};
use crate::sync::manager::pending::PendingDag;

use super::SyncManager;

/// The pending map's estimates, summarized over its roots.
#[derive(Debug, Clone, Default)]
pub(crate) struct PendingDagEstimates {
    pub(crate) attempt_ewma_median_ms: Option<u64>,
    pub(crate) attempt_ewma_max_ms: Option<u64>,
    pub(crate) no_progress_median_ppm: Option<u64>,
    pub(crate) peer_occupancy: Vec<PendingPeerOccupancy>,
}

impl<B: Blockstore + 'static> SyncManager<B> {
    /// Counts the pass when the tie-break would reorder `due`, which is in
    /// dispatch order, and samples per-source occupancy once per tick.
    pub(super) fn observe_dispatch_pass(
        &self,
        now: n0_future::time::Instant,
        due: &[(Cid, PendingDag)],
    ) {
        if let Some(earliest) = due.first().map(|(_, dag)| dag.next_retry_at) {
            let keyed: Vec<_> = due
                .iter()
                .map(|(_, dag)| {
                    (
                        dag.next_retry_at.saturating_duration_since(earliest),
                        dag.estimates.no_progress_q16.unwrap_or(0),
                    )
                })
                .collect();
            if tie_break_reorders(&keyed) {
                self.diagnostics.record_pending_dag_tie_reorder();
            }
        }
        if let Some(tick) = self.pending_occupancy.claim_tick(now) {
            let held = self.pending_dags.read(|pending| pending.held_by_source());
            self.pending_occupancy.sample(tick, &held);
        }
    }

    pub(crate) fn pending_dag_estimates(&self) -> PendingDagEstimates {
        let (mut attempts, mut no_progress, held) = self.pending_dags.read(|pending| {
            let attempts: Vec<u64> = pending
                .values()
                .filter_map(|dag| dag.estimates.attempt_ms.map(u64::from))
                .collect();
            let no_progress: Vec<u64> = pending
                .values()
                .filter_map(|dag| dag.estimates.no_progress_ppm())
                .collect();
            (attempts, no_progress, pending.held_by_source())
        });
        PendingDagEstimates {
            attempt_ewma_median_ms: median(&mut attempts),
            attempt_ewma_max_ms: attempts.iter().copied().max(),
            no_progress_median_ppm: median(&mut no_progress),
            peer_occupancy: self.pending_occupancy.snapshot(&held),
        }
    }
}

/// The middle value, or for an even count the mean of the two middle ones.
fn median(values: &mut [u64]) -> Option<u64> {
    values.sort_unstable();
    let upper = *values.get(values.len() / 2)?;
    if values.len() % 2 == 1 {
        return Some(upper);
    }
    let lower = values.get(values.len() / 2 - 1).copied().unwrap_or(upper);
    Some(lower + (upper - lower) / 2)
}

#[cfg(test)]
mod tests {
    use super::median;

    #[test]
    fn the_median_of_an_even_count_is_the_mean_of_its_middle_two() {
        assert_eq!(median(&mut []), None);
        assert_eq!(median(&mut [7]), Some(7));
        assert_eq!(median(&mut [9, 1, 5]), Some(5));
        assert_eq!(median(&mut [10, 2]), Some(6));
        assert_eq!(median(&mut [u64::MAX, u64::MAX - 2]), Some(u64::MAX - 1));
    }
}
