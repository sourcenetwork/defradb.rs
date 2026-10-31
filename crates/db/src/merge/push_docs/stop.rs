//! The stop owner of one selected push call (`SelectedPushIsolation.tla`).

use std::sync::Arc;

use p2p::error::{AT_CAPACITY_MESSAGE, RATE_LIMITED_MESSAGE};
use p2p::message::PushLogReply;
use regolith::sync::OnceCell;

use crate::merge::push_docs_replay::ReplayPushSendError;

/// One selected push call's stop flag, shared by its caller, the pool
/// crossing, its scheduler and every task it started. Raised once, by the
/// first outcome that proves the transfer cannot progress (a send deadline, a
/// lost connection, a receiver pause hint) or by the caller giving up. Once
/// raised the call starts no more documents and no task sends again; sends
/// already on the wire end on their own deadline and are joined.
#[derive(Clone, Debug, Default)]
pub struct SelectedPushStop(Arc<OnceCell<String>>);

impl SelectedPushStop {
    pub fn new() -> Self {
        Self::default()
    }

    /// Raises the flag; the first `cause` is the one reported.
    pub fn stop(&self, cause: impl Into<String>) {
        let _ = self.0.set(cause.into());
    }

    /// Why the call stopped, once it has.
    pub fn cause(&self) -> Option<&str> {
        self.0.get().map(String::as_str)
    }

    pub fn is_stopped(&self) -> bool {
        self.0.get().is_some()
    }

    /// A guard that raises the flag when dropped, held by a caller whose
    /// drop would otherwise leave started work scheduling the rest.
    pub fn stop_on_drop(&self) -> StopOnDrop {
        StopOnDrop(self.clone())
    }
}

/// Raises its [`SelectedPushStop`] when dropped.
#[must_use = "dropping the guard stops the call"]
pub struct StopOnDrop(SelectedPushStop);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop("the caller stopped waiting");
    }
}

/// Whether a selected send's failure proves the transfer cannot progress now
/// (`DocumentPush.lean` `stopsTransfer`): its deadline passed, the connection
/// is gone (the classifier the retry sweep stops a peer's pass on), or the
/// receiver asked for a pause. Any other failure is one head's own, like a
/// refusal, and leaves the remaining documents to be tried.
pub(crate) fn stops_selected_transfer(error: &ReplayPushSendError) -> bool {
    match error {
        ReplayPushSendError::Timeout { .. }
        | ReplayPushSendError::Backpressure { .. }
        | ReplayPushSendError::SemaphoreClosed => true,
        ReplayPushSendError::Transport(error) => error.is_connection_like(),
        ReplayPushSendError::Persistence(_) | ReplayPushSendError::Stopped => false,
    }
}

/// Whether a rejected reply is the receiver's backpressure rather than a
/// refusal of this one head.
pub(crate) fn reply_requests_pause(reply: &PushLogReply) -> bool {
    matches!(
        reply.err_message.as_deref(),
        Some(message) if message == RATE_LIMITED_MESSAGE || message == AT_CAPACITY_MESSAGE
    )
}
