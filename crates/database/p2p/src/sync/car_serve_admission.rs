//! Admission to the CAR-serve lane (#2536).
//!
//! At most `capacity` CAR serves run at once. A request that finds the lane
//! full waits a short, bounded time in a waiting room instead of being shed at
//! once; a freed slot goes to the waiting request whose peer holds the fewest
//! serves, the oldest first among equals, so one catching-up peer cannot keep
//! a released slot from the others. Active serves are not capped per peer: the
//! requester's `max_concurrent_dag_fetches` bounds how many one peer can start,
//! and only the handoff of a released slot is fair. A request whose wait runs
//! out, or that finds the waiting room (or its peer's share of it) full, is
//! shed and answered busy.
//!
//! The dispatcher's drain owns this value; nothing else touches it. Modeled in
//! `proofs/tla/CarServeAdmission.tla`; the bounds are proved in
//! `proofs/lean/CarServeAdmission.lean`.

use std::collections::VecDeque;
use std::hash::Hash;

use rapidhash::{HashMapExt, RapidHashMap};

/// Divisor for the share of the waiting room one peer may fill.
pub(crate) const CAR_SERVE_WAIT_SHARE_DIVISOR: usize = 4;

/// What became of an offered request.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ServeOffer<T> {
    /// A slot was free: serve it now.
    Admitted(T),
    /// The lane is full: the request waits in the room.
    Waiting,
    /// The lane and the room (or the peer's share of it) are full: answer busy.
    Shed(T),
}

struct Waiter<K, T, I> {
    peer: K,
    item: T,
    deadline: I,
}

/// The lane's slots and its waiting room, keyed by requesting peer.
pub(crate) struct ServeAdmission<K, T, I> {
    capacity: usize,
    waiting_capacity: usize,
    peer_waiting_capacity: usize,
    active_total: usize,
    active: RapidHashMap<K, usize>,
    waiting: VecDeque<Waiter<K, T, I>>,
}

impl<K, T, I> ServeAdmission<K, T, I>
where
    K: Eq + Hash + Clone,
    I: Ord + Copy,
{
    /// A lane of `capacity` slots (at least one) with a waiting room of the
    /// same size, of which one peer may fill at most one
    /// [`CAR_SERVE_WAIT_SHARE_DIVISOR`] (at least one).
    pub(crate) fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            waiting_capacity: capacity,
            peer_waiting_capacity: (capacity / CAR_SERVE_WAIT_SHARE_DIVISOR).max(1),
            active_total: 0,
            active: RapidHashMap::with_capacity(capacity),
            waiting: VecDeque::with_capacity(capacity),
        }
    }

    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }

    #[cfg(test)]
    pub(crate) fn active(&self) -> usize {
        self.active_total
    }

    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// Offer `peer`'s request. A request waits only while the lane is full,
    /// so a free slot never sits beside a waiting request.
    pub(crate) fn offer(&mut self, peer: K, item: T, deadline: I) -> ServeOffer<T> {
        if self.active_total < self.capacity {
            self.take_slot(peer);
            return ServeOffer::Admitted(item);
        }
        let peer_waiting = self.waiting.iter().filter(|w| w.peer == peer).count();
        if self.waiting.len() >= self.waiting_capacity || peer_waiting >= self.peer_waiting_capacity
        {
            return ServeOffer::Shed(item);
        }
        self.waiting.push_back(Waiter {
            peer,
            item,
            deadline,
        });
        ServeOffer::Waiting
    }

    /// Return `peer`'s slot. The slot goes straight to the waiting request
    /// whose peer holds the fewest serves (the oldest among equals), which is
    /// returned to be served, already counted against the lane.
    pub(crate) fn release(&mut self, peer: &K) -> Option<(K, T)> {
        if let Some(held) = self.active.get_mut(peer) {
            *held -= 1;
            if *held == 0 {
                self.active.remove(peer);
            }
            self.active_total -= 1;
        } else {
            debug_assert!(
                false,
                "a CAR serve slot was released by a peer holding none"
            );
            return None;
        }
        let next = self
            .waiting
            .iter()
            .enumerate()
            .min_by_key(|(index, waiter)| (self.held_by(&waiter.peer), *index))
            .map(|(index, _)| index)?;
        let waiter = self.waiting.remove(next)?;
        self.take_slot(waiter.peer.clone());
        Some((waiter.peer, waiter.item))
    }

    /// The earliest deadline among waiting requests.
    pub(crate) fn next_deadline(&self) -> Option<I> {
        self.waiting.iter().map(|waiter| waiter.deadline).min()
    }

    /// Remove and return one waiting request whose deadline is at or before
    /// `now`, to be answered busy.
    pub(crate) fn pop_expired(&mut self, now: I) -> Option<T> {
        let index = self
            .waiting
            .iter()
            .position(|waiter| waiter.deadline <= now)?;
        self.waiting.remove(index).map(|waiter| waiter.item)
    }

    fn held_by(&self, peer: &K) -> usize {
        self.active.get(peer).copied().unwrap_or(0)
    }

    fn take_slot(&mut self, peer: K) {
        *self.active.entry(peer).or_insert(0) += 1;
        self.active_total += 1;
    }
}
