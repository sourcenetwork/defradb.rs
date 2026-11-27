//! Backoff between the round trips of one batch: exponential, capped, and
//! jittered so callers that failed together do not retry together.

use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rapidhash::fast::RandomState;

use crate::config::Config;

/// A lock-free source of jitter: splitmix64 over a shared counter. Callers
/// that share one draw different values, so their waits differ.
pub struct Jitter(AtomicU64);

impl Jitter {
    /// A jitter whose draws follow from `seed`: tests pass a fixed one.
    pub fn new(seed: u64) -> Self {
        Self(AtomicU64::new(seed))
    }

    /// A jitter seeded from a fresh random hasher seed.
    pub fn random() -> Self {
        Self::new(RandomState::default().hash_one(()))
    }

    fn next(&self) -> u64 {
        let mut z = self
            .0
            .fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed)
            .wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform pick between half of `full` and all of it, to the
    /// microsecond.
    pub fn pause(&self, full: Duration) -> Duration {
        let full = u64::try_from(full.as_micros()).unwrap_or(u64::MAX);
        let half = full / 2;
        Duration::from_micros(half + self.next() % (full - half + 1))
    }

    /// The wait before retry `retry` (1 for the first): `backoff * 2^(retry-1)`
    /// capped at `max_backoff`, then a uniform pick between half and all of it.
    pub(crate) fn backoff(&self, config: &Config, retry: u32) -> Duration {
        let base = u64::try_from(config.backoff.as_micros()).unwrap_or(u64::MAX);
        let cap = u64::try_from(config.max_backoff.as_micros()).unwrap_or(u64::MAX);
        let full = base
            .saturating_mul(1u64 << retry.saturating_sub(1).min(32))
            .min(cap);
        self.pause(Duration::from_micros(full))
    }
}
