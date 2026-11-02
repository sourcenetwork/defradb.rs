//! Orders a root's fetch providers by how they have been answering. Ranking
//! only reorders, and keeps the context's order while the announcing peer is
//! healthy; proved in `proofs/lean/ProviderHealth/Ranking.lean`.
//!
//! Known limit: `DagFetchLimiter`'s per-peer permit is keyed on the announcer,
//! so nothing caps the load shed onto one alternate.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use n0_future::time::Instant;
use regolith::sync::HopscotchMap;

use crate::transport::{PeerFetchStats, PeerId};

const MIN_SAMPLES: u64 = 16;
const DEMOTE_DELIVERY_PPM: u64 = 500_000;
const DEMOTE_TIMEOUT_PPM: u64 = 500_000;

/// Every this-many roots keep the context's order so a demoted peer's averages
/// can recover.
const EXPLORE_EVERY: u64 = 16;

/// Matches the retry hint a saturated receiver sends with its at-capacity reply.
const COOLDOWN_BASE: Duration = Duration::from_secs(2);
const COOLDOWN_MAX: Duration = Duration::from_secs(30);
const MAX_COOLDOWN_PEERS: usize = 1024;

#[derive(Debug, Clone, Copy)]
struct Cooldown {
    until: Instant,
    delay: Duration,
}

impl Cooldown {
    /// A fast failure after a stale cool-down starts over instead of doubling.
    fn is_stale(&self, now: Instant) -> bool {
        now >= self.until + self.delay
    }
}

#[derive(Debug)]
pub(crate) struct ProviderHealth {
    cooldowns: HopscotchMap<PeerId, Cooldown, rapidhash::fast::RandomState>,
    roots: AtomicU64,
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self {
            cooldowns: HopscotchMap::with_hasher(rapidhash::fast::RandomState::default()),
            roots: AtomicU64::new(0),
        }
    }
}

/// `demoted` and `cooling` count only peers a healthy peer is asked before.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Ranking {
    pub(crate) peers: Vec<PeerId>,
    pub(crate) demoted: usize,
    pub(crate) cooling: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Standing {
    Healthy,
    Cooling,
    Demoted,
}

impl ProviderHealth {
    /// Healthy, then cooling, then demoted, each in the context's order. When
    /// the announcing peer is moved back the healthy group is rotated per root,
    /// so the load it sheds is spread rather than handed to one alternate.
    pub(crate) fn rank(
        &self,
        providers: Vec<PeerId>,
        stats: &[PeerFetchStats],
        now: Instant,
    ) -> Ranking {
        let turn = self.roots.fetch_add(1, Ordering::Relaxed);
        if (turn + 1).is_multiple_of(EXPLORE_EVERY) {
            return Ranking {
                peers: providers,
                demoted: 0,
                cooling: 0,
            };
        }
        let mut standings: Vec<(Standing, PeerId)> = providers
            .into_iter()
            .map(|peer| (self.standing(&peer, stats, now), peer))
            .collect();
        let source_moved = standings
            .first()
            .is_some_and(|(standing, _)| *standing != Standing::Healthy);
        standings.sort_by_key(|(standing, _)| *standing);
        let healthy = standings
            .iter()
            .take_while(|(standing, _)| *standing == Standing::Healthy)
            .count();
        if source_moved && healthy > 1 {
            standings[..healthy].rotate_left((turn % healthy as u64) as usize);
        }
        let skipped = |wanted| {
            if healthy == 0 {
                return 0;
            }
            standings.iter().filter(|(s, _)| *s == wanted).count()
        };
        Ranking {
            demoted: skipped(Standing::Demoted),
            cooling: skipped(Standing::Cooling),
            peers: standings.into_iter().map(|(_, peer)| peer).collect(),
        }
    }

    fn standing(&self, peer: &PeerId, stats: &[PeerFetchStats], now: Instant) -> Standing {
        // A size-capped CAR is recorded as `Partial`, yet it delivered blocks.
        let failing = stats.iter().any(|stats| {
            stats.peer == peer.as_str()
                && stats.fetches >= MIN_SAMPLES
                && (stats.success_ppm.saturating_add(stats.partial_ppm) < DEMOTE_DELIVERY_PPM
                    || stats.timeout_ppm > DEMOTE_TIMEOUT_PPM)
        });
        if failing {
            Standing::Demoted
        } else if self.cooldowns.get(peer).is_some_and(|c| now < c.until) {
            Standing::Cooling
        } else {
            Standing::Healthy
        }
    }

    pub(crate) fn record_fast_failure(&self, peer: &PeerId, now: Instant) {
        let delay = match self.cooldowns.get(peer) {
            Some(previous) if !previous.is_stale(now) => (previous.delay * 2).min(COOLDOWN_MAX),
            Some(_) => COOLDOWN_BASE,
            None if self.cooldowns.len() >= MAX_COOLDOWN_PEERS && !self.forget_stale(now) => {
                return;
            }
            None => COOLDOWN_BASE,
        };
        self.cooldowns.insert(
            peer.clone(),
            Cooldown {
                until: now + delay,
                delay,
            },
        );
    }

    /// True when forgetting stale cool-downs left room for another.
    fn forget_stale(&self, now: Instant) -> bool {
        let stale: Vec<PeerId> = self
            .cooldowns
            .iter()
            .filter(|(_, cooldown)| cooldown.is_stale(now))
            .map(|(peer, _)| peer)
            .collect();
        for peer in &stale {
            self.cooldowns.remove(peer);
        }
        self.cooldowns.len() < MAX_COOLDOWN_PEERS
    }

    pub(crate) fn record_productive(&self, peer: &PeerId) {
        self.cooldowns.remove(peer);
    }
}

#[cfg(test)]
#[path = "provider_health_tests.rs"]
mod tests;
