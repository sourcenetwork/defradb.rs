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

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use futures::FutureExt;

use crate::rt::mpsc;
use crate::rt::mpsc::error::TryRecvError;
use regolith::sync::{ArrayQueue, Notify};

use super::events::SyncEvent;

/// The receiver clock's dispatches to the merge loop. Every dispatch holds
/// one of the coordinator's fetch slots until the merge loop takes it, so a
/// lane sized by those slots always has room for the next one.
pub(crate) struct DispatchLane {
    queue: ArrayQueue<SyncEvent>,
    ready: Notify,
    queued_or_entering: AtomicUsize,
}

impl DispatchLane {
    pub(crate) fn new(slots: usize) -> Self {
        Self {
            queue: ArrayQueue::new(slots.max(1)),
            ready: Notify::new(),
            queued_or_entering: AtomicUsize::new(0),
        }
    }

    pub(crate) fn has_room(&self) -> bool {
        !self.queue.is_full()
    }

    pub(crate) fn capacity(&self) -> usize {
        self.queue.capacity()
    }

    pub(crate) fn len(&self) -> usize {
        self.queued_or_entering.load(Ordering::Relaxed)
    }

    /// Queues `event`; `false`, dropping it, when the lane is full.
    pub(crate) fn push(&self, event: SyncEvent) -> bool {
        self.queued_or_entering.fetch_add(1, Ordering::Relaxed);
        if self.queue.push(event).is_err() {
            self.queued_or_entering.fetch_sub(1, Ordering::Relaxed);
            return false;
        }
        self.ready.notify_one();
        true
    }

    fn pop(&self) -> Option<SyncEvent> {
        let event = self.queue.pop()?;
        self.queued_or_entering.fetch_sub(1, Ordering::Relaxed);
        Some(event)
    }

    async fn next(&self) -> SyncEvent {
        loop {
            if let Some(event) = self.pop() {
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
        let (dispatch, event) = futures::select_biased! {
            event = self.dispatch.next().fuse() => (true, Some(event)),
            event = self.events.recv().fuse() => (false, event),
        };
        event.map(|event| {
            if dispatch {
                self.dispatched(event)
            } else {
                self.merge_work(event)
            }
        })
    }

    /// The next event, if one is waiting.
    pub fn try_recv(&mut self) -> Result<SyncEvent, TryRecvError> {
        if self.turns == 0 {
            return match self.events.try_recv() {
                Ok(event) => Ok(self.merge_work(event)),
                Err(error) => match self.dispatch.pop() {
                    Some(event) => Ok(self.dispatched(event)),
                    None => Err(error),
                },
            };
        }
        if let Some(event) = self.dispatch.pop() {
            return Ok(self.dispatched(event));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn event(label: &str) -> SyncEvent {
        SyncEvent::SyncError {
            cid: cid::Cid::new_v1(
                0x55,
                cid::multihash::Multihash::<64>::wrap(0, b"event").unwrap(),
            ),
            error: label.to_owned(),
        }
    }

    fn label(event: SyncEvent) -> String {
        match event {
            SyncEvent::SyncError { error, .. } => error,
            _ => panic!("expected a sync error"),
        }
    }

    #[test]
    fn occupancy_tracks_successful_and_rejected_pushes() {
        let dispatch = DispatchLane::new(1);
        assert_eq!((dispatch.len(), dispatch.capacity()), (0, 1));
        assert!(dispatch.push(event("accepted")));
        assert_eq!(dispatch.len(), 1);
        assert!(!dispatch.push(event("rejected")));
        assert_eq!(dispatch.len(), 1);
        assert_eq!(label(dispatch.pop().unwrap()), "accepted");
        assert_eq!(dispatch.len(), 0);
    }

    #[test]
    fn exhausted_lane_yields_to_work_but_drains_when_work_is_empty_or_closed() {
        let dispatch = Arc::new(DispatchLane::new(1));
        let (sender, events) = mpsc::channel(1);
        let mut receiver = SyncEventReceiver::new(Arc::clone(&dispatch), events);

        assert!(dispatch.push(event("first dispatch")));
        sender.try_send(event("merge work")).unwrap();
        assert_eq!(label(receiver.try_recv().unwrap()), "first dispatch");
        assert!(dispatch.push(event("second dispatch")));
        assert_eq!(label(receiver.try_recv().unwrap()), "merge work");
        assert_eq!(label(receiver.try_recv().unwrap()), "second dispatch");

        assert!(dispatch.push(event("empty channel")));
        assert_eq!(label(receiver.try_recv().unwrap()), "empty channel");
        assert!(dispatch.push(event("closed channel")));
        drop(sender);
        assert_eq!(label(receiver.try_recv().unwrap()), "closed channel");
        assert!(matches!(
            receiver.try_recv(),
            Err(TryRecvError::Disconnected)
        ));
    }
}
