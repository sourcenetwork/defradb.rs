//! How each peer answers our CAR fetches: how fast, how reliably, and how much
//! is in flight. Measured only; nothing here changes how we fetch.
//!
//! The rolling averages and totals are proved to stay in range in
//! `proofs/lean/FetchStats/Rolling.lean`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use n0_future::time::Instant;
use regolith::sync::HopscotchMap;

use crate::transport::{PeerFetchStats, PeerId};

/// Peers measured at once. Fetches from any later peer still run, unmeasured.
const MAX_TRACKED_PEERS: usize = 1024;

/// A ratio of 1, in parts per million.
const SCALE: u64 = 1_000_000;

/// A new fetch counts for one part in `WEIGHT` of a rolling average.
const WEIGHT: u64 = 8;

/// A response faster than this has no meaningful delivery rate.
const MIN_RATE_WINDOW: Duration = Duration::from_millis(1);

/// How one fetch ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchOutcome {
    Success,
    /// The provider sent some blocks but hit its response size limit.
    Partial,
    TimedOut,
    Failed,
}

/// Fetch measurements for every peer we fetch from.
#[derive(Debug)]
pub struct FetchStats {
    peers: HopscotchMap<PeerId, Arc<PeerCounters>, rapidhash::fast::RandomState>,
}

impl Default for FetchStats {
    fn default() -> Self {
        Self {
            peers: HopscotchMap::with_hasher(rapidhash::fast::RandomState::default()),
        }
    }
}

impl FetchStats {
    /// Starts measuring one fetch from `peer`.
    pub fn start(&self, peer: &PeerId) -> FetchTimer {
        let counters = self.counters(peer);
        if let Some(counters) = &counters {
            counters.in_flight.fetch_add(1, Ordering::Relaxed);
        }
        FetchTimer {
            counters,
            delivery: None,
            bytes: 0,
            rtt: None,
        }
    }

    fn counters(&self, peer: &PeerId) -> Option<Arc<PeerCounters>> {
        if let Some(counters) = self.peers.get(peer) {
            return Some(counters);
        }
        if self.peers.len() >= MAX_TRACKED_PEERS {
            return None;
        }
        Some(self.peers.get_or_insert(peer.clone(), Arc::default()))
    }

    pub fn snapshot(&self) -> Vec<PeerFetchStats> {
        self.peers
            .iter()
            .map(|(peer, counters)| counters.snapshot(&peer))
            .collect()
    }
}

#[derive(Debug)]
struct PeerCounters {
    fetches: AtomicU64,
    bytes: AtomicU64,
    rate_samples: AtomicU64,
    delivery_rate: AtomicU64,
    /// `u64::MAX` until a round-trip time has been seen.
    min_rtt_micros: AtomicU64,
    success_ppm: AtomicU64,
    timeout_ppm: AtomicU64,
    partial_ppm: AtomicU64,
    in_flight: AtomicU64,
    in_flight_bytes: AtomicU64,
}

impl Default for PeerCounters {
    fn default() -> Self {
        Self {
            fetches: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            rate_samples: AtomicU64::new(0),
            delivery_rate: AtomicU64::new(0),
            min_rtt_micros: AtomicU64::new(u64::MAX),
            success_ppm: AtomicU64::new(0),
            timeout_ppm: AtomicU64::new(0),
            partial_ppm: AtomicU64::new(0),
            in_flight: AtomicU64::new(0),
            in_flight_bytes: AtomicU64::new(0),
        }
    }
}

impl PeerCounters {
    fn record(&self, outcome: FetchOutcome, bytes: u64, rate: Option<u64>, rtt: Option<Duration>) {
        let first = bump(&self.fetches, 1) == 0;
        bump(&self.bytes, bytes);
        let hit = |wanted| if outcome == wanted { SCALE } else { 0 };
        roll_in(&self.success_ppm, first, hit(FetchOutcome::Success));
        roll_in(&self.timeout_ppm, first, hit(FetchOutcome::TimedOut));
        roll_in(&self.partial_ppm, first, hit(FetchOutcome::Partial));
        if let Some(rate) = rate {
            let first_rate = bump(&self.rate_samples, 1) == 0;
            roll_in(&self.delivery_rate, first_rate, rate);
        }
        if let Some(rtt) = rtt {
            let micros = u64::try_from(rtt.as_micros()).unwrap_or(u64::MAX - 1);
            self.min_rtt_micros.fetch_min(micros, Ordering::Relaxed);
        }
    }

    fn snapshot(&self, peer: &PeerId) -> PeerFetchStats {
        let load = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        let min_rtt = load(&self.min_rtt_micros);
        PeerFetchStats {
            peer: peer.to_string(),
            fetches: load(&self.fetches),
            bytes: load(&self.bytes),
            delivery_rate_bytes_per_sec: load(&self.delivery_rate),
            min_rtt_micros: (min_rtt != u64::MAX).then_some(min_rtt),
            success_ppm: load(&self.success_ppm),
            timeout_ppm: load(&self.timeout_ppm),
            partial_ppm: load(&self.partial_ppm),
            in_flight: load(&self.in_flight),
            in_flight_bytes: load(&self.in_flight_bytes),
        }
    }
}

/// One fetch being measured. Dropping it without [`finish`](Self::finish),
/// as a cancelled fetch does, only takes it out of flight.
#[derive(Debug)]
pub struct FetchTimer {
    counters: Option<Arc<PeerCounters>>,
    /// When the first and the latest response bytes arrived.
    delivery: Option<(Instant, Instant)>,
    bytes: u64,
    rtt: Option<Duration>,
}

impl FetchTimer {
    /// `len` more response bytes arrived.
    pub fn received(&mut self, len: usize) {
        let now = Instant::now();
        self.delivery.get_or_insert((now, now)).1 = now;
        let len = len as u64;
        self.bytes = self.bytes.saturating_add(len);
        if let Some(counters) = &self.counters {
            counters.in_flight_bytes.fetch_add(len, Ordering::Relaxed);
        }
    }

    /// The connection's round-trip time while this fetch ran.
    pub fn saw_rtt(&mut self, rtt: Option<Duration>) {
        self.rtt = rtt.or(self.rtt);
    }

    pub fn finish(self, outcome: FetchOutcome) {
        let Some(counters) = &self.counters else {
            return;
        };
        let rate = self
            .delivery
            .map(|(first, last)| last - first)
            .filter(|window| *window >= MIN_RATE_WINDOW)
            .map(|window| {
                let rate = u128::from(self.bytes) * 1_000_000 / window.as_micros();
                u64::try_from(rate).unwrap_or(u64::MAX)
            });
        counters.record(outcome, self.bytes, rate, self.rtt);
    }
}

impl Drop for FetchTimer {
    fn drop(&mut self) {
        if let Some(counters) = &self.counters {
            counters.in_flight.fetch_sub(1, Ordering::Relaxed);
            counters
                .in_flight_bytes
                .fetch_sub(self.bytes, Ordering::Relaxed);
        }
    }
}

/// The round-trip time of the path a connection is using now.
pub fn selected_path_rtt(connection: &iroh::endpoint::Connection) -> Option<Duration> {
    connection
        .paths()
        .iter()
        .find(|path| path.is_selected())
        .map(|path| path.rtt())
}

/// Adds to a total without wrapping, returning the total before.
fn bump(total: &AtomicU64, add: u64) -> u64 {
    total
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |total| {
            Some(total.saturating_add(add))
        })
        .unwrap_or_else(|total| total)
}

/// Rolls a sample into an average; the first sample is taken as is.
fn roll_in(avg: &AtomicU64, first: bool, sample: u64) {
    let _ = avg.try_update(Ordering::Relaxed, Ordering::Relaxed, |avg| {
        Some(if first { sample } else { roll(avg, sample) })
    });
}

const fn roll(avg: u64, sample: u64) -> u64 {
    avg - avg / WEIGHT + sample / WEIGHT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(id: &str) -> PeerId {
        PeerId::new(id.to_string())
    }

    fn only(stats: &FetchStats) -> PeerFetchStats {
        let mut snapshot = stats.snapshot();
        assert_eq!(snapshot.len(), 1);
        snapshot.remove(0)
    }

    #[test]
    fn ratios_follow_outcomes_and_stay_within_one() {
        let stats = FetchStats::default();
        stats.start(&peer("a")).finish(FetchOutcome::Success);
        assert_eq!(only(&stats).success_ppm, SCALE);

        for _ in 0..100 {
            stats.start(&peer("a")).finish(FetchOutcome::TimedOut);
        }
        let snapshot = only(&stats);
        assert_eq!(snapshot.fetches, 101);
        assert!(snapshot.success_ppm < SCALE / 100);
        assert!(snapshot.timeout_ppm > SCALE * 99 / 100);
        assert!(snapshot.timeout_ppm <= SCALE);
        assert_eq!(snapshot.partial_ppm, 0);
    }

    #[test]
    fn a_cancelled_fetch_leaves_flight_without_counting() {
        let stats = FetchStats::default();
        let mut timer = stats.start(&peer("a"));
        timer.received(10);
        assert_eq!(only(&stats).in_flight, 1);
        assert_eq!(only(&stats).in_flight_bytes, 10);

        drop(timer);
        let snapshot = only(&stats);
        assert_eq!((snapshot.in_flight, snapshot.in_flight_bytes), (0, 0));
        assert_eq!(snapshot.fetches, 0);
    }

    #[test]
    fn the_rate_window_ends_at_the_last_byte_not_at_finish() {
        let stats = FetchStats::default();
        let mut timer = stats.start(&peer("a"));
        timer.received(1_000);
        let now = Instant::now();
        timer.delivery = Some((now - Duration::from_secs(3), now - Duration::from_secs(2)));
        timer.finish(FetchOutcome::Success);
        assert_eq!(only(&stats).delivery_rate_bytes_per_sec, 1_000);
    }

    #[test]
    fn the_lowest_rtt_is_kept() {
        let stats = FetchStats::default();
        assert_eq!(only_after(&stats, Duration::from_millis(40)), Some(40_000));
        assert_eq!(only_after(&stats, Duration::from_millis(90)), Some(40_000));
        assert_eq!(only_after(&stats, Duration::from_millis(20)), Some(20_000));
    }

    fn only_after(stats: &FetchStats, rtt: Duration) -> Option<u64> {
        let mut timer = stats.start(&peer("a"));
        timer.saw_rtt(Some(rtt));
        timer.finish(FetchOutcome::Success);
        only(stats).min_rtt_micros
    }

    #[test]
    fn peers_past_the_cap_are_fetched_from_but_not_measured() {
        let stats = FetchStats::default();
        for n in 0..MAX_TRACKED_PEERS {
            stats
                .start(&peer(&n.to_string()))
                .finish(FetchOutcome::Success);
        }
        stats.start(&peer("late")).finish(FetchOutcome::Success);
        assert_eq!(stats.snapshot().len(), MAX_TRACKED_PEERS);
    }

    #[test]
    fn the_largest_samples_do_not_overflow_an_average() {
        // Debug builds panic on overflow, so reaching the asserts is the check.
        assert_eq!(roll(u64::MAX, u64::MAX), u64::MAX);
        assert_eq!(roll(0, u64::MAX), u64::MAX / 8);
        assert_eq!(roll(u64::MAX, 0), u64::MAX - u64::MAX / 8);
    }
}
