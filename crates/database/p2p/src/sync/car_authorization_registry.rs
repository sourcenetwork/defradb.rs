use std::sync::Arc;
use std::time::Duration;

use blockstore::Blockstore;
use cid::Cid;
use rapidhash::fast::RandomState;
use rapidhash::{HashSetExt, RapidHashSet};
use regolith::sync::{ArrayQueue, Atom, Event, HopscotchMap};

use super::budget::{Budget, Reservation};
use super::{WalkState, AUTHORIZATION_MAX_NODES, AUTHORIZATION_TIMEOUT};
use crate::error::{Error, Result};

const RETAINED_INCOMPLETE_WALKS: usize = 64;
const RETAINED_MAX_CIDS: usize = 262_144;
pub(super) const CURSOR_IDLE_TTL: Duration = Duration::from_secs(300);

#[derive(Clone, Eq, PartialEq, Hash)]
pub(super) struct WalkKey {
    pub(super) root: Cid,
    requested: Vec<Cid>,
}

struct SavedWalk {
    state: WalkState,
    _cids: Reservation,
    _slot: Reservation,
}

pub(super) struct CursorEntry {
    value: ArrayQueue<SavedWalk>,
    last_used: crate::rt::time::Instant,
    #[cfg(test)]
    pub(super) snapshot: CursorSnapshot,
}

#[cfg(test)]
pub(super) struct CursorSnapshot {
    pub(super) frontier_len: usize,
    pub(super) remaining_capacity: usize,
    pub(super) cost: usize,
}

impl CursorEntry {
    fn new(state: WalkState, cids: Reservation, slot: Reservation) -> Self {
        #[cfg(test)]
        let snapshot = CursorSnapshot {
            frontier_len: state.queue.len(),
            remaining_capacity: state.remaining.capacity(),
            cost: state.seen.len() + state.remaining.len() + state.authorized.len(),
        };
        let value = ArrayQueue::new(1);
        assert!(value
            .push(SavedWalk {
                state,
                _cids: cids,
                _slot: slot
            })
            .is_ok());
        Self {
            value,
            last_used: crate::rt::time::Instant::now(),
            #[cfg(test)]
            snapshot,
        }
    }

    fn take(&self) -> Option<WalkState> {
        let SavedWalk {
            state,
            _cids,
            _slot,
        } = self.value.pop()?;
        drop((_cids, _slot));
        Some(state)
    }

    fn discard(&self) {
        drop(self.value.pop());
    }
}

enum FlightOutcome {
    Pending,
    Complete(RapidHashSet<Cid>),
    Failed,
}

struct Flight {
    done: Event,
    outcome: Atom<FlightOutcome>,
}

impl Flight {
    fn new() -> Self {
        Self {
            done: Event::new(),
            outcome: Atom::new(FlightOutcome::Pending),
        }
    }

    async fn join(&self, budget: Duration) -> Result<RapidHashSet<Cid>> {
        crate::rt::time::timeout(budget, self.done.wait())
            .await
            .map_err(|_| Error::ResponseTimeout)?;
        match &*self.outcome.load() {
            FlightOutcome::Complete(grants) => Ok(grants.clone()),
            FlightOutcome::Pending | FlightOutcome::Failed => Err(Error::ResponseTimeout),
        }
    }
}

/// Idle state is taken out while walking; duplicates join a bounded completion event.
pub(crate) struct RootedAuthorizationProgress {
    pub(super) cursors: HopscotchMap<WalkKey, Arc<CursorEntry>, RandomState>,
    flights: HopscotchMap<WalkKey, Arc<Flight>, RandomState>,
    pub(super) cid_budget: Arc<Budget>,
    slots: Arc<Budget>,
}

impl Default for RootedAuthorizationProgress {
    fn default() -> Self {
        Self::with_limit(RETAINED_MAX_CIDS)
    }
}

impl RootedAuthorizationProgress {
    fn with_limit(limit: usize) -> Self {
        Self {
            cursors: HopscotchMap::with_hasher(RandomState::default()),
            flights: HopscotchMap::with_hasher(RandomState::default()),
            cid_budget: Budget::new(limit),
            slots: Budget::new(RETAINED_INCOMPLETE_WALKS),
        }
    }

    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(super) fn with_retention_limit(limit: usize) -> Self {
        Self::with_limit(limit)
    }

    pub(crate) async fn walk<B>(
        &self,
        blockstore: &B,
        root: Cid,
        requested: RapidHashSet<Cid>,
    ) -> Result<RapidHashSet<Cid>>
    where
        B: Blockstore,
    {
        self.walk_with_limits(
            blockstore,
            root,
            requested,
            AUTHORIZATION_TIMEOUT,
            AUTHORIZATION_MAX_NODES,
        )
        .await
    }

    pub(crate) async fn walk_with_limits<B>(
        &self,
        blockstore: &B,
        root: Cid,
        requested: RapidHashSet<Cid>,
        budget: Duration,
        max_nodes: usize,
    ) -> Result<RapidHashSet<Cid>>
    where
        B: Blockstore,
    {
        if requested.is_empty() {
            return Ok(RapidHashSet::new());
        }
        let mut wanted: Vec<_> = requested.iter().copied().collect();
        wanted.sort_unstable();
        let key = WalkKey {
            root,
            requested: wanted,
        };
        let flight = Arc::new(Flight::new());
        if let Some(owner) = self.flights.insert_if_absent(key.clone(), flight.clone()) {
            return owner.join(budget).await;
        }
        self.expire_idle();
        let state = self
            .cursors
            .remove(&key)
            .and_then(|entry| entry.take())
            .unwrap_or_else(|| WalkState::fresh(root, requested));
        let mut walk = RunningWalk {
            progress: self,
            key,
            flight,
            state: Some(state),
        };
        let state = walk.state.as_mut().expect("running walk owns its state");
        let outcome = state.run(blockstore, budget, max_nodes).await;
        let retain = outcome.is_err() || (!state.remaining.is_empty() && state.can_resume());
        let result = outcome.map(|()| state.authorized.clone());
        walk.finish(&result, retain);
        result
    }

    fn expire_idle(&self) {
        for (key, entry) in self.cursors.iter() {
            if entry.last_used.elapsed() >= CURSOR_IDLE_TTL {
                self.remove_observed(&key, &entry);
            }
        }
    }

    pub(super) fn remove_observed(&self, key: &WalkKey, observed: &Arc<CursorEntry>) -> bool {
        if let Some(entry) = self
            .cursors
            .remove_if(key, |current| Arc::ptr_eq(current, observed))
        {
            entry.discard();
            true
        } else {
            false
        }
    }

    fn evict_oldest(&self) -> bool {
        let Some((key, entry)) = self.cursors.iter().min_by_key(|(_, entry)| entry.last_used)
        else {
            return false;
        };
        self.remove_observed(&key, &entry)
    }

    fn retain(&self, key: WalkKey, state: WalkState) {
        let cost = state
            .seen
            .len()
            .saturating_add(state.remaining.len())
            .saturating_add(state.authorized.len());
        if cost > self.cid_budget.limit {
            return;
        }
        for _ in 0..=RETAINED_INCOMPLETE_WALKS {
            let cids = self.cid_budget.reserve(cost);
            let slot = self.slots.reserve(1);
            if let (Some(cids), Some(slot)) = (cids, slot) {
                let entry = Arc::new(CursorEntry::new(state, cids, slot));
                if self.cursors.insert_if_absent(key, entry.clone()).is_some() {
                    entry.discard();
                }
                return;
            }
            if !self.evict_oldest() {
                return;
            }
        }
    }
}

struct RunningWalk<'progress> {
    progress: &'progress RootedAuthorizationProgress,
    key: WalkKey,
    flight: Arc<Flight>,
    state: Option<WalkState>,
}

impl RunningWalk<'_> {
    fn finish(&mut self, result: &Result<RapidHashSet<Cid>>, retain: bool) {
        if let Some(state) = self.state.take().filter(|_| retain) {
            self.progress.retain(self.key.clone(), state);
        }
        self.flight.outcome.store(match result {
            Ok(grants) => FlightOutcome::Complete(grants.clone()),
            Err(_) => FlightOutcome::Failed,
        });
        self.release();
    }

    fn release(&self) {
        self.progress
            .flights
            .remove_if(&self.key, |current| Arc::ptr_eq(current, &self.flight));
        self.flight.done.set();
    }
}

impl Drop for RunningWalk<'_> {
    fn drop(&mut self) {
        if self.flight.done.is_set() {
            return;
        }
        if let Some(state) = self.state.take().filter(WalkState::can_resume) {
            self.progress.retain(self.key.clone(), state);
        }
        self.flight.outcome.store(FlightOutcome::Failed);
        self.release();
    }
}
