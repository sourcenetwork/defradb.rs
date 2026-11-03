//! Per-source occupancy of the pending-DAG map: entries held, in Q8, rolled in
//! once per `OCCUPANCY_TICK` at weight `2^-OCCUPANCY_SHIFT`. One packed word
//! per source, `[tick + 1 : 40 | entries · 256 : 24]`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use n0_future::time::Instant;
use rapidhash::RapidHashMap;
use regolith::sync::HopscotchMap;

use super::ewma::{decode, encode};
use super::{millis_since, roll, OCCUPANCY_SHIFT, OCCUPANCY_TICK, VALUE_MAX};

/// Sources measured at once; the pending map's own cap bounds them first.
const MAX_TRACKED_SOURCES: usize = 1024;

/// Ticks rolled one by one after a gap; a longer gap starts over from the
/// sample, which the missed ticks would have converged to anyway.
const MAX_CATCH_UP_TICKS: u64 = 1024;

/// An estimate below half an entry is dropped once its source holds nothing.
const DROP_BELOW_Q8: u64 = 128;

/// How much of the pending map one source peer holds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PendingPeerOccupancy {
    pub peer: String,
    /// Entries it holds now.
    pub held: usize,
    /// Entries it held on average over the last few minutes, times 256.
    pub occupancy_q8: u64,
}

#[derive(Debug)]
pub(crate) struct PendingOccupancy {
    epoch: Instant,
    /// The tick last sampled, plus one.
    sampled: AtomicU64,
    sources: HopscotchMap<Arc<str>, u64, rapidhash::fast::RandomState>,
}

impl Default for PendingOccupancy {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            sampled: AtomicU64::new(0),
            sources: HopscotchMap::with_hasher(rapidhash::fast::RandomState::default()),
        }
    }
}

impl PendingOccupancy {
    /// The tick `now` falls in, when no caller has sampled it yet. Exactly one
    /// caller wins a tick, so each source gets one sample per tick.
    pub(crate) fn claim_tick(&self, now: Instant) -> Option<u64> {
        let tick_ms = u64::try_from(OCCUPANCY_TICK.as_millis())
            .unwrap_or(u64::MAX)
            .max(1);
        let tick = millis_since(self.epoch, now) / tick_ms;
        let stamp = tick.saturating_add(1);
        let last = self.sampled.load(Ordering::Relaxed);
        (stamp > last
            && self
                .sampled
                .compare_exchange(last, stamp, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok())
        .then_some(tick)
    }

    /// Rolls each source's held count into its estimate for `tick`; a tracked
    /// source that holds nothing now rolls in zero.
    pub(crate) fn sample(&self, tick: u64, held: &RapidHashMap<Arc<str>, usize>) {
        for (source, count) in held {
            let word = self.sources.get(source);
            if word.is_none() && self.sources.len() >= MAX_TRACKED_SOURCES {
                continue;
            }
            let entries = u64::try_from(*count).unwrap_or(u64::MAX);
            let next = advance(word, tick, entries.saturating_mul(256));
            self.sources.insert(Arc::clone(source), next);
        }
        for (source, word) in self.sources.iter() {
            if held.contains_key(&source) {
                continue;
            }
            let next = advance(Some(word), tick, 0);
            if decode(next).is_none_or(|estimate| estimate.value < DROP_BELOW_Q8) {
                self.sources.remove(&source);
            } else {
                self.sources.insert(source, next);
            }
        }
    }

    pub(crate) fn snapshot(
        &self,
        held: &RapidHashMap<Arc<str>, usize>,
    ) -> Vec<PendingPeerOccupancy> {
        let mut peers: Vec<PendingPeerOccupancy> = self
            .sources
            .iter()
            .map(|(source, word)| PendingPeerOccupancy {
                held: held.get(&source).copied().unwrap_or_default(),
                occupancy_q8: decode(word).map_or(0, |estimate| estimate.value),
                peer: source.to_string(),
            })
            .collect();
        peers.sort_by(|a, b| a.peer.cmp(&b.peer));
        peers
    }
}

/// The word after `tick`'s sample: the first is taken as is; after a gap of
/// `k` ticks the sample is rolled in `k` times, as if held throughout. A tick
/// earlier than the last reads as no gap.
pub(super) fn advance(word: Option<u64>, tick: u64, sample: u64) -> u64 {
    let sample = sample.min(VALUE_MAX);
    let value = match word.and_then(decode) {
        None => sample,
        Some(last) => {
            let ticks = tick.saturating_sub(last.at);
            if ticks >= MAX_CATCH_UP_TICKS {
                sample
            } else {
                (0..ticks).fold(last.value, |avg, _| roll(avg, sample, OCCUPANCY_SHIFT))
            }
        }
    };
    encode(tick, value)
}
