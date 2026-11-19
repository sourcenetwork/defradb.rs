//! How one input's service behaves.

use std::time::Duration;

/// A service's deadline, retries, batches, in-flight cap and cache.
///
/// Every bound is a count of the resource it bounds: the cache by bytes,
/// round trips by how many are in flight, batches by keys per round trip.
#[derive(Clone, Debug)]
pub struct Config {
    /// The longest a caller waits for an answer, from its call, retries
    /// included; also the budget of one batch's round trips and retries.
    pub deadline: Duration,
    /// Round trips one batch makes at most: the first and its retries.
    pub attempts: u32,
    /// The wait before the first retry. Each later retry doubles it, up to
    /// `max_backoff`; each wait is jittered between half and all of it.
    pub backoff: Duration,
    /// The longest wait between two round trips of one batch.
    pub max_backoff: Duration,
    /// The most keys one round trip carries.
    pub max_batch: usize,
    /// The most round trips one group has in flight at once. With 1, keys
    /// that arrive while a round trip is in flight ride the next one; a
    /// source without a batch call sets `max_batch` to 1 and this to the
    /// concurrency it allows.
    pub flights_per_group: usize,
    /// The most round trips in flight at once, over every group.
    pub max_in_flight: usize,
    /// The bytes the cache holds at most; 0 caches nothing.
    pub cache_bytes: usize,
    /// How long a cached answer stays valid; `None` keeps it until it is
    /// evicted or invalidated.
    pub ttl: Option<Duration>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(30),
            attempts: 3,
            backoff: Duration::from_millis(50),
            max_backoff: Duration::from_secs(2),
            max_batch: 256,
            flights_per_group: 1,
            max_in_flight: 64,
            cache_bytes: 0,
            ttl: None,
        }
    }
}

impl Config {
    /// The bounds a service actually runs with: every count at least 1.
    pub(crate) fn normalized(mut self) -> Self {
        self.attempts = self.attempts.max(1);
        self.max_batch = self.max_batch.max(1);
        self.flights_per_group = self.flights_per_group.max(1);
        self.max_in_flight = self.max_in_flight.max(1);
        self.max_backoff = self.max_backoff.max(self.backoff);
        self
    }
}
