//! A service's metrics: plain atomics, read as a snapshot.

use std::sync::atomic::{AtomicU64, Ordering};

/// Buckets of a [`Histogram`]: bucket `i` counts values below `2^i`.
const BUCKETS: usize = 64;

/// A histogram over powers of two, written with atomics only.
#[derive(Debug)]
pub struct Histogram {
    buckets: [AtomicU64; BUCKETS],
    count: AtomicU64,
    sum: AtomicU64,
    max: AtomicU64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

impl Histogram {
    /// An empty histogram; usable in a `static`.
    pub const fn new() -> Self {
        Self {
            buckets: [const { AtomicU64::new(0) }; BUCKETS],
            count: AtomicU64::new(0),
            sum: AtomicU64::new(0),
            max: AtomicU64::new(0),
        }
    }

    /// Counts `value`.
    pub fn record(&self, value: u64) {
        let bucket = (u64::BITS - value.leading_zeros()) as usize;
        self.buckets[bucket.min(BUCKETS - 1)].fetch_add(1, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(value, Ordering::Relaxed);
        self.max.fetch_max(value, Ordering::Relaxed);
    }

    /// Values counted.
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Their sum.
    pub fn sum(&self) -> u64 {
        self.sum.load(Ordering::Relaxed)
    }

    /// The largest, or 0 when none was counted.
    pub fn max(&self) -> u64 {
        self.max.load(Ordering::Relaxed)
    }

    /// An upper bound on the `q` quantile (`0.0..=1.0`): the top of the
    /// bucket it falls in, or `None` when nothing was counted.
    pub fn quantile(&self, q: f64) -> Option<u64> {
        let count = self.count();
        if count == 0 {
            return None;
        }
        let rank = ((count as f64) * q.clamp(0.0, 1.0)).ceil().max(1.0) as u64;
        let mut seen = 0;
        for (bucket, counter) in self.buckets.iter().enumerate() {
            seen += counter.load(Ordering::Relaxed);
            if seen >= rank {
                let top = if bucket == 0 { 0 } else { (1u64 << bucket) - 1 };
                return Some(top.min(self.max()));
            }
        }
        Some(self.max())
    }
}

/// What a service did, since it started.
#[derive(Debug, Default)]
pub struct Stats {
    /// Keys asked for.
    pub requests: AtomicU64,
    /// Keys answered from the cache.
    pub cache_hits: AtomicU64,
    /// Keys that joined a flight already in progress.
    pub joined: AtomicU64,
    /// Keys sent on a round trip.
    pub fetched: AtomicU64,
    /// Round trips made, retries included.
    pub round_trips: AtomicU64,
    /// Round trips that were retries.
    pub retries: AtomicU64,
    /// Batches that ended without answers.
    pub failures: AtomicU64,
    /// Callers that gave up at their deadline.
    pub deadlines: AtomicU64,
    /// Answers cached.
    pub cached: AtomicU64,
    /// Keys per round trip.
    pub batch_sizes: Histogram,
    /// Microseconds per round trip, on the remote side.
    pub round_trip_micros: Histogram,
    /// Microseconds a caller waited, from its call to its answers.
    pub wait_micros: Histogram,
    /// Round trips one call waited on.
    pub trips_per_call: Histogram,
}

impl Stats {
    pub(crate) fn add(counter: &AtomicU64, n: u64) {
        counter.fetch_add(n, Ordering::Relaxed);
    }

    /// The value of `counter`.
    pub fn get(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }
}
