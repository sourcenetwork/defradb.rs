//! Management-channel request/reply correlation.
//!
//! Tracks in-flight management-channel message IDs → response channels so
//! inbound [`ManageReply`] and [`ManageQueryReply`] envelopes (delivered
//! asynchronously over the two-stream protocol) can be routed back to the
//! requester.
//!
//! Two independent correlators are provided:
//! - [`ManageCorrelator`] for mutating operations (ack/error reply via [`ManageReply`]).
//! - [`ManageQueryCorrelator`] for read-only query operations (typed reply via [`ManageQueryReply`]).
//!
//! Both mirror the SE query correlator ([`crate::se_correlator`]) exactly:
//! a shared lock-free map + Drop-cleanup guard, keyed by message-ID `String`.

use tokio::sync::oneshot;
use tracing::debug;

use crate::message::{ManageQueryReply, ManageReply};
use crate::se_correlator::{new_ongoing, register_slot, take_sender, Ongoing};

// ---------------------------------------------------------------------------
// ManageCorrelator: for mutating management operations (ManageReply)
// ---------------------------------------------------------------------------

/// Outstanding manage-request registry shared between the requester and the
/// event loop that receives replies. Cheap to clone into tasks.
#[derive(Clone)]
pub struct ManageCorrelator {
    ongoing: Ongoing<ManageReply>,
}

impl Default for ManageCorrelator {
    fn default() -> Self {
        Self {
            ongoing: new_ongoing(),
        }
    }
}

/// Handle for a registered manage request. The correlator slot is removed when
/// this guard is dropped, so a requester that times out (or returns early) does
/// not leak the entry. Await [`PendingManage::recv`] for the reply (keep the
/// guard alive across the await so the slot stays registered).
pub struct PendingManage {
    message_id: String,
    receiver: oneshot::Receiver<ManageReply>,
    correlator: ManageCorrelator,
}

impl PendingManage {
    /// Await the reply for this request. Returns `Err` if the slot was
    /// cancelled (sender dropped) before a reply arrived.
    pub async fn recv(&mut self) -> std::result::Result<ManageReply, oneshot::error::RecvError> {
        (&mut self.receiver).await
    }
}

impl Drop for PendingManage {
    fn drop(&mut self) {
        self.correlator.cancel(&self.message_id);
    }
}

impl ManageCorrelator {
    /// Create a new, empty correlator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a slot for `message_id` and return a guard whose `receiver`
    /// resolves when the matching reply arrives. Registering the same
    /// `message_id` twice overwrites the first slot (the first caller then
    /// times out); callers must use unique message IDs.
    pub fn register(&self, message_id: String) -> PendingManage {
        let receiver = register_slot(&self.ongoing, message_id.clone());
        PendingManage {
            message_id,
            receiver,
            correlator: self.clone(),
        }
    }

    /// Deliver a reply, routing it to the matching registered slot.
    ///
    /// Returns `true` if a waiting requester received the reply, `false` if
    /// the reply was stale (no matching slot, late arrival, or requester gone).
    pub fn deliver(&self, reply: ManageReply) -> bool {
        match take_sender(&self.ongoing, &reply.message_id) {
            Some(tx) => tx.send(reply).is_ok(),
            None => {
                debug!(message_id = %reply.message_id, "manage: reply with no matching request dropped");
                false
            }
        }
    }

    /// Drop the slot for `message_id` without waiting for a reply.
    pub fn cancel(&self, message_id: &str) {
        drop(take_sender(&self.ongoing, message_id));
    }

    /// Number of currently in-flight manage requests. For tests/metrics only.
    pub fn in_flight(&self) -> usize {
        self.ongoing.len()
    }
}

// ---------------------------------------------------------------------------
// ManageQueryCorrelator: for read-only management operations (ManageQueryReply)
// ---------------------------------------------------------------------------

/// Outstanding manage-query registry shared between the requester and the
/// event loop that receives replies. Cheap to clone into tasks.
#[derive(Clone)]
pub struct ManageQueryCorrelator {
    ongoing: Ongoing<ManageQueryReply>,
}

impl Default for ManageQueryCorrelator {
    fn default() -> Self {
        Self {
            ongoing: new_ongoing(),
        }
    }
}

/// Handle for a registered manage-query request. The correlator slot is
/// removed when this guard is dropped, so a requester that times out (or
/// returns early) does not leak the entry. Await [`PendingManageQuery::recv`]
/// for the reply (keep the guard alive across the await so the slot stays
/// registered).
pub struct PendingManageQuery {
    message_id: String,
    receiver: oneshot::Receiver<ManageQueryReply>,
    correlator: ManageQueryCorrelator,
}

impl PendingManageQuery {
    /// Await the reply for this query. Returns `Err` if the slot was cancelled
    /// (sender dropped) before a reply arrived.
    pub async fn recv(
        &mut self,
    ) -> std::result::Result<ManageQueryReply, oneshot::error::RecvError> {
        (&mut self.receiver).await
    }
}

impl Drop for PendingManageQuery {
    fn drop(&mut self) {
        self.correlator.cancel(&self.message_id);
    }
}

impl ManageQueryCorrelator {
    /// Create a new, empty correlator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a slot for `message_id` and return a guard whose `receiver`
    /// resolves when the matching reply arrives. Registering the same
    /// `message_id` twice overwrites the first slot (the first caller then
    /// times out); callers must use unique message IDs.
    pub fn register(&self, message_id: String) -> PendingManageQuery {
        let receiver = register_slot(&self.ongoing, message_id.clone());
        PendingManageQuery {
            message_id,
            receiver,
            correlator: self.clone(),
        }
    }

    /// Deliver a reply, routing it to the matching registered slot.
    ///
    /// Returns `true` if a waiting requester received the reply, `false` if
    /// the reply was stale (no matching slot, late arrival, or requester gone).
    pub fn deliver(&self, reply: ManageQueryReply) -> bool {
        match take_sender(&self.ongoing, &reply.message_id) {
            Some(tx) => tx.send(reply).is_ok(),
            None => {
                debug!(message_id = %reply.message_id, "manage_query: reply with no matching request dropped");
                false
            }
        }
    }

    /// Drop the slot for `message_id` without waiting for a reply.
    pub fn cancel(&self, message_id: &str) {
        drop(take_sender(&self.ongoing, message_id));
    }

    /// Number of currently in-flight manage-query requests. For tests/metrics only.
    pub fn in_flight(&self) -> usize {
        self.ongoing.len()
    }
}
