//! The two ways a [`SyncEvent`] reaches the merge loop.
//!
//! Merge work (blocks to merge, completed DAGs, errors) queues on a bounded
//! channel whose senders wait for room. The receiver clock's dispatches take
//! a lane of their own, which the merge loop takes from first: a dispatch
//! never competes with a waiting sender for room, and never waits behind more
//! than one event of queued merge work. After as many dispatches in a row as
//! the lane has slots, waiting merge work takes the next turn, so dispatches
//! re-armed before every receive cannot hold it back. Model:
//! proofs/tla/MC_SyncOwnership_Dispatch.tla (`DispatchPath`, `LaneTurns`).

use std::sync::Arc;

use regolith::sync::{ArrayQueue, Notify};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;

use super::events::SyncEvent;

/// The receiver clock's dispatches to the merge loop. Every dispatch holds
/// one of the coordinator's fetch slots until the merge loop takes it, so a
/// lane sized by those slots always has room for the next one.
pub(crate) struct DispatchLane {
    queue: ArrayQueue<SyncEvent>,
    ready: Notify,
}

impl DispatchLane {
    pub(crate) fn new(slots: usize) -> Self {
        Self {
            queue: ArrayQueue::new(slots.max(1)),
            ready: Notify::new(),
        }
    }

    pub(crate) fn has_room(&self) -> bool {
        !self.queue.is_full()
    }

    fn capacity(&self) -> usize {
        self.queue.capacity()
    }

    /// Queues `event`; `false`, dropping it, when the lane is full.
    pub(crate) fn push(&self, event: SyncEvent) -> bool {
        if self.queue.push(event).is_err() {
            return false;
        }
        self.ready.notify_one();
        true
    }

    fn pop(&self) -> Option<SyncEvent> {
        self.queue.pop()
    }

    async fn next(&self) -> SyncEvent {
        loop {
            if let Some(event) = self.queue.pop() {
                return event;
            }
            self.ready.notified().await;
        }
    }
}

/// The merge loop's end of both: the clock's dispatches first, then merge
/// work in the order it was sent. Once the lane has had as many turns in a
/// row as it has slots, waiting merge work goes next.
pub struct SyncEventReceiver {
    dispatch: Arc<DispatchLane>,
    events: mpsc::Receiver<SyncEvent>,
    /// Dispatches the lane may still hand over ahead of waiting merge work.
    turns: usize,
}

impl SyncEventReceiver {
    pub(crate) fn new(dispatch: Arc<DispatchLane>, events: mpsc::Receiver<SyncEvent>) -> Self {
        let turns = dispatch.capacity();
        Self {
            dispatch,
            events,
            turns,
        }
    }

    /// The next event, waiting for one; `None` once the merge-work channel
    /// is closed.
    pub async fn recv(&mut self) -> Option<SyncEvent> {
        if let Ok(event) = self.try_recv() {
            return Some(event);
        }
        tokio::select! {
            biased;
            event = self.dispatch.next() => Some(self.dispatched(event)),
            event = self.events.recv() => event.map(|event| self.merge_work(event)),
        }
    }

    /// The next event, if one is waiting.
    pub fn try_recv(&mut self) -> Result<SyncEvent, TryRecvError> {
        if self.turns > 0 || self.events.is_empty() {
            if let Some(event) = self.dispatch.pop() {
                return Ok(self.dispatched(event));
            }
        }
        self.events.try_recv().map(|event| self.merge_work(event))
    }

    fn dispatched(&mut self, event: SyncEvent) -> SyncEvent {
        self.turns = self.turns.saturating_sub(1);
        event
    }

    fn merge_work(&mut self, event: SyncEvent) -> SyncEvent {
        self.turns = self.dispatch.capacity();
        event
    }
}

#[cfg(test)]
impl From<mpsc::Receiver<SyncEvent>> for SyncEventReceiver {
    fn from(events: mpsc::Receiver<SyncEvent>) -> Self {
        Self::new(Arc::new(DispatchLane::new(1)), events)
    }
}
