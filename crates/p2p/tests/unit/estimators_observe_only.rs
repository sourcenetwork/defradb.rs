//! The estimators observe and never act: whatever they hold, the re-arm after
//! an attempt, the dispatch order of due roots and the provider ranking are
//! the ones the code computes without them.

use crate::sync::arbitration::{AttemptSample, ProviderEstimateExport, RootEstimates};
use crate::sync::coordinator::provider_health::ProviderHealth;
use crate::transport::{PeerFetchStats, PeerId};

use super::*;

fn samples() -> Vec<AttemptSample> {
    let mut samples = vec![
        AttemptSample::Skipped,
        AttemptSample::Complete,
        AttemptSample::Shed { progressed: false },
        AttemptSample::Shed { progressed: true },
        AttemptSample::Preempted,
    ];
    for duration in [Duration::ZERO, Duration::from_millis(300), Duration::MAX] {
        for (fast, progressed) in [(true, false), (false, false), (false, true)] {
            samples.push(AttemptSample::Incomplete {
                duration,
                fast,
                progressed,
            });
        }
    }
    samples
}

fn adversarial(seed: usize) -> RootEstimates {
    RootEstimates {
        attempt_ms: [None, Some(0), Some(1), Some(u32::MAX)][seed % 4],
        no_progress_q16: [None, Some(0), Some(u16::MAX), Some(17)][seed / 4 % 4],
    }
}

#[tokio::test(start_paused = true)]
async fn no_sample_changes_when_a_root_is_due_again() {
    let manager = test_manager();
    let now = n0_future::time::Instant::now();
    for (dispatches, preempted) in (0..7).flat_map(|d| [(d, false), (d, true)]) {
        for (index, sample) in samples().into_iter().enumerate() {
            let offset = index * 8 + usize::from(preempted) * 4 + dispatches as usize * 1_000;
            let (with, without) = (test_cid(40_000 + offset), test_cid(80_000 + offset));
            for (cid, seed) in [(with, index), (without, 0)] {
                let mut dag = pending_dag("observe", Instant::now());
                dag.dispatches = dispatches;
                dag.next_retry_at = now;
                dag.estimates = adversarial(seed);
                manager.insert_pending_dag(cid, dag);
            }
            manager
                .pending_dag_lease(with)
                .finish_attempt(preempted, sample);
            manager
                .pending_dag_lease(without)
                .finish_attempt(preempted, AttemptSample::Skipped);
            let (with_cid, without_cid) = (with, without);
            let (with, without) = (
                manager.pending_dag_snapshot(&with).unwrap(),
                manager.pending_dag_snapshot(&without).unwrap(),
            );
            manager.clear_pending_dag(&with_cid);
            manager.clear_pending_dag(&without_cid);
            assert_eq!(with.next_retry_at, without.next_retry_at, "{sample:?}");
            let rearm = if preempted {
                now
            } else {
                now + retry_backoff(dispatches)
            };
            assert_eq!(with.next_retry_at, rearm);
            assert_eq!(
                (with.dispatches, with.incomplete_attempts),
                (without.dispatches, without.incomplete_attempts)
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn the_due_order_ignores_the_estimates() {
    let manager = test_manager();
    let now = n0_future::time::Instant::now();
    let roots: Vec<Cid> = (0..48).map(|i| test_cid(6_000 + i)).collect();
    for (i, cid) in roots.iter().enumerate() {
        let mut dag = pending_dag("order", Instant::now());
        dag.next_retry_at = now - Duration::from_millis((i as u64 * 7_919) % 3_000);
        dag.estimates = adversarial(i);
        manager.insert_pending_dag(*cid, dag);
    }
    let order =
        |due: Vec<(Cid, PendingDag)>| due.into_iter().map(|(cid, _)| cid).collect::<Vec<_>>();
    let with = order(manager.due_pending_dag_retries(now));
    let mut expected = manager.due_pending_dag_retries(now);
    expected.sort_by(|(a_cid, a), (b_cid, b)| {
        a.next_retry_at
            .cmp(&b.next_retry_at)
            .then_with(|| a_cid.to_bytes().cmp(&b_cid.to_bytes()))
    });
    assert_eq!(with, order(expected));
    assert!(
        manager.diagnostics.snapshot().pending_dag_tie_reorders > 0,
        "the measured tie-break would reorder these roots"
    );

    manager.pending_dags.update(|pending| {
        for cid in &roots {
            if let Some(dag) = pending.get_mut(cid) {
                dag.estimates = RootEstimates::default();
            }
        }
    });
    assert_eq!(order(manager.due_pending_dag_retries(now)), with);
}

#[test]
fn the_provider_ranking_ignores_the_estimates() {
    let peers: Vec<PeerId> = ["source", "a", "b", "c"]
        .into_iter()
        .map(|id| PeerId::new(id.to_string()))
        .collect();
    let stats = |estimates: ProviderEstimateExport| -> Vec<PeerFetchStats> {
        peers
            .iter()
            .enumerate()
            .map(|(i, peer)| PeerFetchStats {
                peer: peer.to_string(),
                fetches: 100,
                success_ppm: [100_000, 900_000, 950_000, 400_000][i],
                timeout_ppm: [0, 0, 600_000, 0][i],
                estimates,
                ..Default::default()
            })
            .collect()
    };
    let extreme = ProviderEstimateExport {
        success_latency_micros: Some(u64::MAX),
        useful_ewma_ppm: Some(0),
        attempt_cost_micros: Some(1),
        expected_useful_micros: Some(u64::MAX),
        ewma_age_millis: Some(0),
    };
    let (plain, measured) = (ProviderHealth::default(), ProviderHealth::default());
    let at = n0_future::time::Instant::now();
    for _ in 0..64 {
        let without = plain.rank(peers.clone(), &stats(ProviderEstimateExport::default()), at);
        let with = measured.rank(peers.clone(), &stats(extreme), at);
        assert_eq!(with, without);
    }
}
