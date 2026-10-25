//! Replies we expect on a new stream. SE query, manage and manage query
//! replies come back on a stream of their own, so a stream with one of those
//! tags is admitted only while we have a request out to that peer. Admission
//! checks this from the tag, before it reads the length or the body. Which
//! request a reply answers is matched later, by message id, in the
//! correlators. Model: `proofs/tla/SolicitedReplies.tla`.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use iroh::EndpointId;
use kovan_queue::seg_queue::SegQueue;
use n0_future::time::{Duration, Instant};
use rapidhash::fast::RandomState;
use regolith::sync::HopscotchMap;

/// How long a reply is expected: twice the longest requester timeout (manage
/// requests, 30 s), so a slow but valid reply is not refused.
const REPLY_WINDOW: Duration = Duration::from_secs(60);
/// Entries kept before expired ones are pruned.
const PRUNE_AT: usize = 1024;

type Key = (EndpointId, &'static [u8]);

struct Expected {
    count: AtomicU32,
    /// Milliseconds since `Solicited::start`.
    deadline: AtomicU64,
}

pub(super) struct Solicited {
    start: Instant,
    entries: HopscotchMap<Key, Arc<Expected>, RandomState>,
    /// Every key in `entries`, for pruning; the map cannot be iterated.
    keys: SegQueue<Key>,
}

impl Solicited {
    pub(super) fn new() -> Self {
        Self {
            start: Instant::now(),
            entries: HopscotchMap::with_hasher(RandomState::default()),
            keys: SegQueue::new(),
        }
    }

    /// Expects one more reply from `peer` on `reply_tag`. Call it before
    /// sending the request, so a fast reply finds it. Only the endpoint task
    /// calls this and `prune`, so a prune never runs between this read and
    /// its write.
    pub(super) fn expect(&self, peer: EndpointId, reply_tag: &'static [u8]) {
        self.expect_at(peer, reply_tag, self.now());
    }

    fn expect_at(&self, peer: EndpointId, reply_tag: &'static [u8], now: u64) {
        if self.entries.len() >= PRUNE_AT {
            self.prune(now);
        }
        let deadline = now + REPLY_WINDOW.as_millis() as u64;
        match self.entries.get(&(peer, reply_tag)) {
            Some(entry) => {
                // An expired entry's leftovers belong to requests whose
                // requesters gave up; they must not admit replies now.
                if entry.deadline.load(Ordering::Acquire) <= now {
                    entry.count.store(1, Ordering::Release);
                } else {
                    entry.count.fetch_add(1, Ordering::AcqRel);
                }
                entry.deadline.store(deadline, Ordering::Release);
            }
            None => {
                let entry = Arc::new(Expected {
                    count: AtomicU32::new(1),
                    deadline: AtomicU64::new(deadline),
                });
                self.entries.insert((peer, reply_tag), entry);
                self.keys.push((peer, reply_tag));
            }
        }
    }

    /// Takes one expected reply from `peer` on `reply_tag`; `false` when none
    /// is left or its window has closed.
    pub(super) fn take(&self, peer: EndpointId, reply_tag: &'static [u8]) -> bool {
        self.take_at(peer, reply_tag, self.now())
    }

    fn take_at(&self, peer: EndpointId, reply_tag: &'static [u8], now: u64) -> bool {
        let Some(entry) = self.entries.get(&(peer, reply_tag)) else {
            return false;
        };
        entry.deadline.load(Ordering::Acquire) > now
            && entry
                .count
                .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                    count.checked_sub(1)
                })
                .is_ok()
    }

    /// Drops the entries whose window has closed.
    fn prune(&self, now: u64) {
        for _ in 0..self.keys.len() {
            let Some(key) = self.keys.pop() else {
                break;
            };
            let expired = self
                .entries
                .get(&key)
                .is_none_or(|entry| entry.deadline.load(Ordering::Acquire) <= now);
            if expired {
                self.entries.remove(&key);
            } else {
                self.keys.push(key);
            }
        }
    }

    fn now(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

#[cfg(test)]
#[path = "../../tests/iroh/solicited.rs"]
mod tests;
