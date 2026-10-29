//! A timer on the runtime's clock that reports a stopping runtime instead
//! of polling a timer driver that is going away.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::error::EdgeError;
use crate::platform::{Instant, Sleep};
use crate::shared::Shared;

/// Stands in for "never" when a limit does not fit the clock.
const FAR_FUTURE: Duration = Duration::from_secs(86_400 * 365 * 30);

pub(crate) struct Timer {
    sleep: Pin<Box<Sleep>>,
    shared: Arc<Shared>,
}

impl Timer {
    pub(crate) fn after(shared: &Arc<Shared>, from: Instant, limit: Duration) -> Self {
        Self {
            sleep: Box::pin(shared.rt.sleep_until(deadline_at(from, limit))),
            shared: Arc::clone(shared),
        }
    }

    /// Ready once `limit` passed, as the deadline error it counts, or once
    /// the runtime is stopping, as [`EdgeError::Closed`].
    pub(crate) fn poll_limit(&mut self, cx: &mut Context<'_>, limit: Duration) -> Poll<EdgeError> {
        self.poll(cx).map(|elapsed| {
            if elapsed {
                self.shared.stats.deadline();
                EdgeError::Deadline(limit)
            } else {
                EdgeError::Closed
            }
        })
    }

    /// Ready with `true` once the time passed, `false` once the runtime is
    /// stopping. A timer of a stopping runtime is not polled: the flag is
    /// checked first, and the catch covers a timer polled while it goes.
    pub(crate) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<bool> {
        if self.shared.is_stopped() {
            return Poll::Ready(false);
        }
        let (rt, sleep) = (&self.shared.rt, self.sleep.as_mut());
        match catch_unwind(AssertUnwindSafe(|| rt.poll_sleep(sleep, cx))) {
            Ok(Poll::Ready(())) => Poll::Ready(true),
            Ok(Poll::Pending) => Poll::Pending,
            Err(_) => Poll::Ready(false),
        }
    }
}

fn deadline_at(from: Instant, limit: Duration) -> Instant {
    from.checked_add(limit).unwrap_or_else(|| from + FAR_FUTURE)
}
