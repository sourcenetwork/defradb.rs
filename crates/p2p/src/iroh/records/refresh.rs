//! When to ask a node for its record: its pointer names a newer one than we
//! hold. Gossip may lose an announcement; this is what still gets the newest
//! record through (`ServiceRecords.tla`, `LIVE_NewestReached`).

use iroh::EndpointId;
use n0_future::time::{Duration, Instant};
use rapidhash::{HashMapExt, RapidHashMap};

use super::{Book, Pointer};

/// How long to wait before asking the same node again.
const ASK_INTERVAL: Duration = Duration::from_secs(30);
/// The most nodes whose last ask we remember; older ones are forgotten.
const MAX_ASKED: usize = 4096;

#[derive(Default)]
pub struct Refresh {
    asked: RapidHashMap<EndpointId, Instant>,
}

impl Refresh {
    pub fn new() -> Self {
        Self {
            asked: RapidHashMap::new(),
        }
    }

    /// Whether to query `node` now, given the pointer it publishes. Records
    /// the ask, so a node is asked at most once per interval.
    pub fn check(&mut self, book: &Book, node: EndpointId, pointer: &Pointer, now: Instant) -> bool {
        if !book.is_behind(&node, pointer) {
            return false;
        }
        if self
            .asked
            .get(&node)
            .is_some_and(|last| now.duration_since(*last) < ASK_INTERVAL)
        {
            return false;
        }
        if self.asked.len() >= MAX_ASKED {
            self.asked.retain(|_, last| now.duration_since(*last) < ASK_INTERVAL);
        }
        if self.asked.len() < MAX_ASKED {
            self.asked.insert(node, now);
        }
        true
    }
}
