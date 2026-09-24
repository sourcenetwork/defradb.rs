//! Searchable-encryption query correlation.
//!
//! Tracks in-flight `QuerySEArtifactsRequest` message IDs → response channels so
//! inbound [`QuerySEArtifactsReply`] envelopes (delivered asynchronously over the
//! SE query response two-stream protocol) can be routed back to the requester.
//!
//! Mirrors the KMS [`crate::pubsub_rpc::correlator::Correlator`] state machine
//! (shared lock-free map + Drop-cleanup), but is keyed by the message-ID
//! `String` rather than a `Cid`, since SE query message IDs are UUID strings.
//! The slot helpers here are shared with [`crate::manage_correlator`].

use std::sync::Arc;

use kovan_map::HopscotchMap;
use kovan_queue::array_queue::ArrayQueue;
use tokio::sync::oneshot;
use tracing::debug;

use crate::message::QuerySEArtifactsReply;

/// One-slot holder for a registered sender: whichever of deliver, cancel,
/// overwrite or drop removes the entry pops the sender out and owns it, so
/// the requester's receiver ends right there rather than when the retired
/// map node is reclaimed.
type ReplySlot<R> = Arc<ArrayQueue<oneshot::Sender<R>>>;

pub(crate) type Ongoing<R> = Arc<HopscotchMap<String, ReplySlot<R>, rapidhash::fast::RandomState>>;

pub(crate) fn new_ongoing<R: Send + 'static>() -> Ongoing<R> {
    Arc::new(HopscotchMap::with_hasher(
        rapidhash::fast::RandomState::default(),
    ))
}

pub(crate) fn register_slot<R: Send + 'static>(
    ongoing: &Ongoing<R>,
    message_id: String,
) -> oneshot::Receiver<R> {
    let (tx, rx) = oneshot::channel();
    let slot = ArrayQueue::new(1);
    let _ = slot.push(tx);
    if let Some(previous) = ongoing.insert(message_id, Arc::new(slot)) {
        drop(previous.pop());
    }
    rx
}

pub(crate) fn take_sender<R: Send + 'static>(
    ongoing: &Ongoing<R>,
    message_id: &str,
) -> Option<oneshot::Sender<R>> {
    ongoing.remove(message_id).and_then(|slot| slot.pop())
}

/// Outstanding SE-query registry shared between the requester and the event
/// loop that receives replies. Cheap to clone into tasks.
#[derive(Clone)]
pub struct SeQueryCorrelator {
    ongoing: Ongoing<QuerySEArtifactsReply>,
}

impl Default for SeQueryCorrelator {
    fn default() -> Self {
        Self {
            ongoing: new_ongoing(),
        }
    }
}

/// Handle for a registered SE query. The correlator slot is removed when this
/// guard is dropped, so a requester that times out (or returns early) does not
/// leak the entry. Await [`PendingSeQuery::recv`] for the reply (keep the guard
/// alive across the await so the slot stays registered).
pub struct PendingSeQuery {
    message_id: String,
    receiver: oneshot::Receiver<QuerySEArtifactsReply>,
    correlator: SeQueryCorrelator,
}

impl PendingSeQuery {
    /// Await the reply for this query. Returns `Err` if the slot was cancelled
    /// (sender dropped) before a reply arrived.
    pub async fn recv(
        &mut self,
    ) -> std::result::Result<QuerySEArtifactsReply, oneshot::error::RecvError> {
        (&mut self.receiver).await
    }
}

impl Drop for PendingSeQuery {
    fn drop(&mut self) {
        self.correlator.cancel(&self.message_id);
    }
}

impl SeQueryCorrelator {
    /// Create a new, empty correlator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a slot for `message_id` and return a guard whose `receiver`
    /// resolves when the matching reply arrives. Registering the same
    /// `message_id` twice overwrites the first slot (the first caller then
    /// times out); callers must use unique message IDs (sign-first generates a
    /// fresh UUID per request).
    pub fn register(&self, message_id: String) -> PendingSeQuery {
        let receiver = register_slot(&self.ongoing, message_id.clone());
        PendingSeQuery {
            message_id,
            receiver,
            correlator: self.clone(),
        }
    }

    /// Deliver a reply, routing it to the matching registered slot.
    ///
    /// Returns `true` if a waiting requester received the reply, `false` if the
    /// reply was stale (no matching slot, late arrival, or requester gone).
    pub fn deliver(&self, reply: QuerySEArtifactsReply) -> bool {
        match take_sender(&self.ongoing, &reply.message_id) {
            Some(tx) => tx.send(reply).is_ok(),
            None => {
                debug!(message_id = %reply.message_id, "se_query: reply with no matching request dropped");
                false
            }
        }
    }

    /// Drop the slot for `message_id` without waiting for a reply.
    pub fn cancel(&self, message_id: &str) {
        drop(take_sender(&self.ongoing, message_id));
    }

    /// Number of currently in-flight SE queries. For tests/metrics only.
    pub fn in_flight(&self) -> usize {
        self.ongoing.len()
    }
}
