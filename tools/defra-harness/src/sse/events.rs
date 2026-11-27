use std::sync::Arc;
use std::time::Duration;

use regolith::sync::{Atom, Notify};
use serde_json::Value;

/// The events one SSE subscription has received, in arrival order.
///
/// Clones share the same events. The subscription's task is the only writer;
/// readers take a snapshot and never wait for it.
#[derive(Clone)]
pub struct SseEvents {
    inner: Arc<Inner>,
}

struct Inner {
    events: Atom<Arc<Vec<Value>>>,
    arrived: Notify,
}

impl SseEvents {
    pub(super) fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                events: Atom::new(Arc::new(Vec::new())),
                arrived: Notify::new(),
            }),
        }
    }

    pub(super) fn push(&self, event: Value) {
        let mut events = Vec::clone(&self.snapshot());
        events.push(event);
        self.inner.events.store(Arc::new(events));
        self.inner.arrived.notify_waiters();
    }

    /// The events received so far.
    pub fn snapshot(&self) -> Arc<Vec<Value>> {
        self.inner.events.load_clone()
    }

    /// How many events have been received so far.
    pub fn len(&self) -> usize {
        self.snapshot().len()
    }

    /// Whether no event has been received yet.
    pub fn is_empty(&self) -> bool {
        self.snapshot().is_empty()
    }

    /// The events once `enough` holds for them, or `Err` with the events
    /// received when `timeout` ran out first.
    pub async fn wait_until<F>(
        &self,
        timeout: Duration,
        mut enough: F,
    ) -> Result<Arc<Vec<Value>>, Arc<Vec<Value>>>
    where
        F: FnMut(&[Value]) -> bool,
    {
        let wait = async {
            loop {
                let arrived = self.inner.arrived.notified();
                let events = self.snapshot();
                if enough(&events) {
                    return events;
                }
                arrived.await;
            }
        };
        tokio::time::timeout(timeout, wait)
            .await
            .map_err(|_| self.snapshot())
    }
}
