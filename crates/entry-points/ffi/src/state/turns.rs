//! The turns of one explicit transaction, so the calls a C caller makes on it
//! run one at a time, in the order they were made, even when a call returns
//! before its work runs (the asynchronous query).
//!
//! A call takes the next ticket as it names the transaction, waits until the
//! transaction serves that ticket, runs, and serves the next one when its
//! ticket drops. A ticket dropped before its turn is marked, and whoever
//! reaches it (the call finishing before it, or the one dropping it) skips it,
//! removing the mark in one step so it is skipped exactly once.
//!
//! What must never happen: two calls running at once, a call overtaking an
//! earlier one (a commit landing before work asked for first), or the
//! transaction stalling on a ticket nobody skips. Modeled by
//! `proofs/tla/TxnTurns.tla` and `proofs/lean/TxnTurns.lean`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use regolith::sync::Notify;

/// One transaction's turns.
pub struct Turns {
    /// The next ticket number to hand out.
    next: AtomicU64,
    /// The ticket whose turn it is.
    serving: AtomicU64,
    /// Tickets dropped before their turn and not yet skipped.
    dropped: HopscotchMap<u64, (), RandomState>,
    /// Wakes the calls waiting for their turn when the served ticket moves.
    changed: Notify,
}

impl Turns {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            next: AtomicU64::new(0),
            serving: AtomicU64::new(0),
            dropped: HopscotchMap::with_hasher(RandomState::default()),
            changed: Notify::new(),
        })
    }

    /// The next ticket, in the order calls ask for one.
    pub fn take(self: &Arc<Self>) -> Ticket {
        Ticket {
            turns: Arc::clone(self),
            number: self.next.fetch_add(1, Ordering::AcqRel),
            started: AtomicBool::new(false),
        }
    }

    /// Ticket `number` is done: serve the next one, skipping every ticket
    /// dropped before its turn, and wake whoever waits.
    fn finish(&self, number: u64) {
        self.serving.store(number + 1, Ordering::Release);
        loop {
            let serving = self.serving.load(Ordering::Acquire);
            // Removing the mark is what claims the skip: of a finisher and a
            // dropping call that both reach it, only one removes it.
            if self.dropped.remove(&serving).is_none() {
                break;
            }
            self.serving.store(serving + 1, Ordering::Release);
        }
        self.changed.notify_waiters();
    }
}

/// A call's place in its transaction's turns. Dropping it serves the next
/// ticket, after its turn, or marks it skipped, before.
pub struct Ticket {
    turns: Arc<Turns>,
    number: u64,
    /// Set once its turn came and its call started.
    started: AtomicBool,
}

impl Ticket {
    /// Waits until the transaction serves this ticket.
    pub async fn wait(&self) {
        loop {
            // Asked for before the check, so a move between the check and the
            // await still wakes it.
            let changed = self.turns.changed.notified();
            if self.turns.serving.load(Ordering::Acquire) == self.number {
                self.started.store(true, Ordering::Release);
                return;
            }
            changed.await;
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if self.started.load(Ordering::Acquire) {
            self.turns.finish(self.number);
            return;
        }
        // Dropped before its turn: mark it, then look again, since the call
        // before it may have looked for the mark before it was there.
        self.turns.dropped.insert(self.number, ());
        if self.turns.serving.load(Ordering::Acquire) == self.number
            && self.turns.dropped.remove(&self.number).is_some()
        {
            self.turns.finish(self.number);
        }
    }
}

#[cfg(test)]
#[path = "turns_tests.rs"]
mod tests;
