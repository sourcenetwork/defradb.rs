//! A panic in network work becomes that work's error and a count, never a
//! failure of anything else.

use std::any::Any;

use crate::error::EdgeError;
use crate::stats::Stats;

/// Turns the outcome of a `catch_unwind` around network work into the
/// work's result, counting and logging a panic once, where it happened.
pub(crate) fn contain<T>(stats: &Stats, outcome: std::thread::Result<T>) -> Result<T, EdgeError> {
    outcome.map_err(|payload| {
        let message = panic_message(payload.as_ref());
        stats.panicked();
        tracing::error!(panic = %message, "network work panicked; only its own completion fails");
        EdgeError::Panicked(message)
    })
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "a panic with a non-text payload".to_string()
    }
}
