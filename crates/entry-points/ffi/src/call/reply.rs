//! The caller's callback for an asynchronous call, called exactly once.

use crate::types::DefraResult;

/// Receives the result of an asynchronous call, exactly once.
///
/// - `context` is the value the caller passed with the call.
/// - `result` is the call's result, as its synchronous form returns it: the
///   caller owns it and frees it with `defra_result_free`.
///
/// The callback runs on a DefraDB thread that serves other work: it must
/// return promptly, and a synchronous DefraDB call made from it is refused
/// (an asynchronous one is fine). A call that fails before it is submitted
/// calls it on the caller's own thread, before the call returns.
pub type DefraResultCallback = unsafe extern "C" fn(context: usize, result: DefraResult);

/// What a call has not reported when it ends without a result.
pub(super) const ERR_ENDED: &str =
    "the call ended without a result: it panicked, or the node shut down";

/// An asynchronous call's way back to its caller. Dropping it unsent (the
/// call's task panicked or was dropped at shutdown, or it was never
/// admitted) reports that, so the callback runs exactly once either way.
pub(crate) struct Reply {
    callback: DefraResultCallback,
    context: usize,
    sent: bool,
}

impl Reply {
    pub(crate) fn new(callback: DefraResultCallback, context: usize) -> Self {
        Self {
            callback,
            context,
            sent: false,
        }
    }

    /// Hands `result` to the caller.
    pub(crate) fn send(mut self, result: DefraResult) {
        self.sent = true;
        self.call(result);
    }

    fn call(&self, result: DefraResult) {
        // SAFETY: the caller's callback contract: it takes the context it
        // gave and owns the result from here.
        unsafe { (self.callback)(self.context, result) }
    }
}

impl Drop for Reply {
    fn drop(&mut self) {
        if !self.sent {
            self.call(DefraResult::error(ERR_ENDED));
        }
    }
}
