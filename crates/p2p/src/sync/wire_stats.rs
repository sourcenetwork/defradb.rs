//! Counters a transport keeps for its own node: gossip payloads that would
//! not decode, and Go Shinzo's wire traffic (batches received, CARs built to
//! serve, CARs fetched or missed). The transport owns one `WireStats` per
//! node and hands it to the sync coordinator through
//! [`P2PTransport::wire_stats`](crate::transport::P2PTransport::wire_stats),
//! so two nodes in one process never see each other's counts.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use kovan::Atom;

const GOSSIP_DECODE_FAILURE_SAMPLE_LIMIT: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GossipTransport {
    Iroh,
    Libp2p,
}

impl GossipTransport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Iroh => "iroh",
            Self::Libp2p => "libp2p",
        }
    }
}

impl std::fmt::Display for GossipTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GossipDecodeFailureSample {
    pub transport: GossipTransport,
    pub peer_id: String,
    pub topic: String,
    pub message_size: usize,
    pub error: String,
    pub payload_fingerprint: String,
    pub payload_shape_hint: String,
    pub occurrences: u64,
}

/// One node's transport-level counters.
#[derive(Debug)]
pub struct WireStats {
    gossip_decode_failures: AtomicU64,
    /// A bounded, deduplicated ring of recent samples, replaced as a unit on
    /// every (rare) decode failure.
    recent_gossip_decode_failures: Atom<VecDeque<GossipDecodeFailureSample>>,
    shinzo_seen: AtomicBool,
    batches: AtomicU64,
    car_built: AtomicU64,
    car_fetched: AtomicU64,
    car_fetch_missed: AtomicU64,
}

impl Default for WireStats {
    fn default() -> Self {
        Self {
            gossip_decode_failures: AtomicU64::new(0),
            recent_gossip_decode_failures: Atom::new(VecDeque::with_capacity(
                GOSSIP_DECODE_FAILURE_SAMPLE_LIMIT,
            )),
            shinzo_seen: AtomicBool::new(false),
            batches: AtomicU64::new(0),
            car_built: AtomicU64::new(0),
            car_fetched: AtomicU64::new(0),
            car_fetch_missed: AtomicU64::new(0),
        }
    }
}

impl WireStats {
    /// Record a gossip payload decode failure, keep a deduplicated sample,
    /// and return the node's new total, so callers can warn on the 1st, 2nd,
    /// 4th, 8th... failure.
    pub fn record_gossip_decode_failure(&self, sample: GossipDecodeFailureSample) -> u64 {
        let total = self.gossip_decode_failures.fetch_add(1, Ordering::Relaxed) + 1;
        self.recent_gossip_decode_failures.rcu(|current| {
            let mut recent = current.clone();
            let matched = recent.iter().position(|existing| {
                existing.transport == sample.transport
                    && existing.peer_id == sample.peer_id
                    && existing.topic == sample.topic
                    && existing.payload_fingerprint == sample.payload_fingerprint
                    && existing.payload_shape_hint == sample.payload_shape_hint
            });
            match matched.and_then(|index| recent.remove(index)) {
                Some(mut existing) => {
                    existing.message_size = sample.message_size;
                    existing.error = sample.error.clone();
                    existing.occurrences += 1;
                    recent.push_back(existing);
                }
                None => {
                    recent.push_back(GossipDecodeFailureSample {
                        occurrences: 1,
                        ..sample.clone()
                    });
                    while recent.len() > GOSSIP_DECODE_FAILURE_SAMPLE_LIMIT {
                        recent.pop_front();
                    }
                }
            }
            recent
        });
        total
    }

    /// Every gossip decode failure this node has seen.
    pub fn gossip_decode_failures(&self) -> u64 {
        self.gossip_decode_failures.load(Ordering::Relaxed)
    }

    /// The most recent distinct decode failures, oldest first.
    pub fn recent_gossip_decode_failures(&self) -> Vec<GossipDecodeFailureSample> {
        self.recent_gossip_decode_failures
            .load()
            .iter()
            .cloned()
            .collect()
    }

    fn bump(&self, counter: &AtomicU64, by: u64) {
        self.shinzo_seen.store(true, Ordering::Relaxed);
        counter.fetch_add(by, Ordering::Relaxed);
    }

    /// A batched Go Shinzo push arrived (`batches`).
    pub fn record_batch(&self) {
        self.bump(&self.batches, 1);
    }

    /// `count` CARs were built to answer a Go CAR request (`carBuilt`).
    pub fn record_car_built(&self, count: u64) {
        self.bump(&self.car_built, count);
    }

    /// A Go CAR reply carried the head's CAR (`carFetched`) or did not
    /// (`carFetchMissed`).
    pub fn record_car_fetch(&self, fetched: bool) {
        let counter = if fetched {
            &self.car_fetched
        } else {
            &self.car_fetch_missed
        };
        self.bump(counter, 1);
    }

    /// This interval's Shinzo wire counts by Go name, each reset to zero;
    /// empty until this node has used the Shinzo wire.
    pub fn drain_shinzo(&self) -> Vec<(&'static str, u64)> {
        if !self.shinzo_seen.load(Ordering::Relaxed) {
            return Vec::new();
        }
        [
            ("batches", &self.batches),
            ("carBuilt", &self.car_built),
            ("carFetched", &self.car_fetched),
            ("carFetchMissed", &self.car_fetch_missed),
        ]
        .into_iter()
        .map(|(name, counter)| (name, counter.swap(0, Ordering::Relaxed)))
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(peer: &str) -> GossipDecodeFailureSample {
        GossipDecodeFailureSample {
            transport: GossipTransport::Libp2p,
            peer_id: peer.into(),
            topic: "topic".into(),
            message_size: 3,
            error: "bad".into(),
            payload_fingerprint: "fp".into(),
            payload_shape_hint: "map".into(),
            occurrences: 0,
        }
    }

    #[test]
    fn shinzo_counts_appear_once_used_and_reset_each_interval() {
        let stats = WireStats::default();
        assert!(stats.drain_shinzo().is_empty());
        stats.record_batch();
        stats.record_car_built(3);
        stats.record_car_fetch(true);
        stats.record_car_fetch(false);
        stats.record_car_fetch(false);
        assert_eq!(
            stats.drain_shinzo(),
            vec![
                ("batches", 1),
                ("carBuilt", 3),
                ("carFetched", 1),
                ("carFetchMissed", 2)
            ]
        );
        assert!(stats.drain_shinzo().iter().all(|(_, n)| *n == 0));
    }

    #[test]
    fn decode_failures_are_counted_and_sampled_without_repeats() {
        let stats = WireStats::default();
        assert_eq!(stats.record_gossip_decode_failure(sample("a")), 1);
        assert_eq!(stats.record_gossip_decode_failure(sample("a")), 2);
        assert_eq!(stats.record_gossip_decode_failure(sample("b")), 3);
        let recent = stats.recent_gossip_decode_failures();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].peer_id, "a");
        assert_eq!(recent[0].occurrences, 2);
        for peer in 0..20 {
            stats.record_gossip_decode_failure(sample(&peer.to_string()));
        }
        assert_eq!(
            stats.recent_gossip_decode_failures().len(),
            GOSSIP_DECODE_FAILURE_SAMPLE_LIMIT
        );
    }

    #[test]
    fn two_nodes_keep_separate_counts() {
        let (first, second) = (WireStats::default(), WireStats::default());
        first.record_batch();
        first.record_gossip_decode_failure(sample("a"));
        assert_eq!(second.drain_shinzo(), Vec::new());
        assert_eq!(second.gossip_decode_failures(), 0);
        assert_eq!(first.drain_shinzo()[0], ("batches", 1));
    }
}
