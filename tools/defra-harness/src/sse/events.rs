use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use regolith::sync::{Atom, Notify};
use serde_json::Value;

/// Why a waiter stopped before its event condition held.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SseWaitError {
    /// The subscription failed before the expected events arrived.
    #[error("SSE stream failed: {message}")]
    Stream {
        message: Arc<str>,
        events: Arc<Vec<Value>>,
    },
    /// The event condition did not hold before the deadline.
    #[error("timed out after {timeout:?} with {count} events")]
    Timeout {
        timeout: Duration,
        count: usize,
        events: Arc<Vec<Value>>,
    },
}

impl SseWaitError {
    /// The events received before the failure.
    pub fn events(&self) -> &[Value] {
        match self {
            Self::Stream { events, .. } | Self::Timeout { events, .. } => events,
        }
    }

    /// The transport or HTTP failure, when the stream failed.
    pub fn stream_error(&self) -> Option<&str> {
        match self {
            Self::Stream { message, .. } => Some(message),
            Self::Timeout { .. } => None,
        }
    }
}

/// The events one SSE subscription has received, in arrival order.
///
/// Clones share the same events. The subscription's task appends one node in
/// constant time; readers walk the immutable nodes and take a snapshot.
#[derive(Clone)]
pub struct SseEvents {
    inner: Arc<Inner>,
}

struct Inner {
    head: Atom<Option<Arc<EventNode>>>,
    len: AtomicUsize,
    error: Atom<Option<Arc<str>>>,
    arrived: Notify,
}

struct EventNode {
    event: Value,
    previous: Option<Arc<EventNode>>,
}

impl Drop for EventNode {
    fn drop(&mut self) {
        let mut next = self.previous.take();
        while let Some(node) = next {
            match Arc::try_unwrap(node) {
                Ok(mut node) => next = node.previous.take(),
                Err(_) => break,
            }
        }
    }
}

impl SseEvents {
    pub(super) fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                head: Atom::new(None),
                len: AtomicUsize::new(0),
                error: Atom::new(None),
                arrived: Notify::new(),
            }),
        }
    }

    pub(super) fn push(&self, event: Value) {
        let previous = self.inner.head.load_clone();
        self.inner
            .head
            .store(Some(Arc::new(EventNode { event, previous })));
        self.inner.len.fetch_add(1, Ordering::Release);
        self.inner.arrived.notify_waiters();
    }

    pub(super) fn fail(&self, message: impl Into<Arc<str>>) {
        self.inner.error.store(Some(message.into()));
        self.inner.arrived.notify_waiters();
    }

    /// The events received so far.
    pub fn snapshot(&self) -> Arc<Vec<Value>> {
        let mut events = Vec::with_capacity(self.inner.len.load(Ordering::Acquire));
        let mut node = self.inner.head.load_clone();
        while let Some(current) = node {
            events.push(current.event.clone());
            node = current.previous.clone();
        }
        events.reverse();
        Arc::new(events)
    }

    /// The terminal stream error, if the subscription failed.
    pub fn error(&self) -> Option<Arc<str>> {
        self.inner.error.load_clone()
    }

    /// How many events have been received so far.
    pub fn len(&self) -> usize {
        self.inner.len.load(Ordering::Acquire)
    }

    /// Whether no event has been received yet.
    pub fn is_empty(&self) -> bool {
        self.inner.head.load_clone().is_none()
    }

    /// The events once `enough` holds for them, or why the wait ended first.
    pub async fn wait_until<F>(
        &self,
        timeout: Duration,
        mut enough: F,
    ) -> Result<Arc<Vec<Value>>, SseWaitError>
    where
        F: FnMut(&[Value]) -> bool,
    {
        let wait = async {
            loop {
                let arrived = self.inner.arrived.notified();
                let events = self.snapshot();
                if enough(&events) {
                    return Ok(events);
                }
                if let Some(message) = self.error() {
                    return Err(SseWaitError::Stream { message, events });
                }
                arrived.await;
            }
        };
        match tokio::time::timeout(timeout, wait).await {
            Ok(result) => result,
            Err(_) => {
                let events = self.snapshot();
                match self.error() {
                    Some(message) => Err(SseWaitError::Stream { message, events }),
                    None => Err(SseWaitError::Timeout {
                        timeout,
                        count: events.len(),
                        events,
                    }),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_event_chain_drops_on_a_small_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let events = SseEvents::new();
                for n in 0..50_000 {
                    events.push(serde_json::json!(n));
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[tokio::test]
    async fn a_waiter_wakes_when_events_arrive() {
        let events = SseEvents::new();
        let writer = events.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            writer.push(serde_json::json!(1));
            writer.push(serde_json::json!(2));
        });
        let got = events
            .wait_until(Duration::from_secs(2), |e| e.len() >= 2)
            .await
            .unwrap();
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn pushes_preserve_order_without_rewriting_the_snapshot() {
        let events = SseEvents::new();
        events.push(serde_json::json!({ "n": 1 }));
        let first = events.snapshot();
        events.push(serde_json::json!({ "n": 2 }));

        assert_eq!(first.as_slice(), &[serde_json::json!({ "n": 1 })]);
        assert_eq!(
            events.snapshot().as_slice(),
            &[serde_json::json!({ "n": 1 }), serde_json::json!({ "n": 2 })]
        );
    }

    #[test]
    fn a_terminal_error_is_retained_for_waiters() {
        let events = SseEvents::new();
        events.fail("connection reset");

        assert_eq!(events.error().as_deref(), Some("connection reset"));
    }

    #[tokio::test]
    async fn a_waiter_reports_the_terminal_stream_error() {
        let events = SseEvents::new();
        events.fail("connection reset");

        let failure = events
            .wait_until(Duration::from_secs(1), |_| false)
            .await
            .unwrap_err();

        assert_eq!(failure.stream_error(), Some("connection reset"));
    }
}
