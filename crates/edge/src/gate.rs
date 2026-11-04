//! Admission and drain.
//!
//! A gate admits work until it closes, counting what it admitted, and a
//! closer can wait for that count to reach zero. Gates nest: a scope's gate
//! admits only what its parent also admits, so closing the runtime closes
//! every scope, while closing a scope leaves the runtime and the other
//! scopes accepting.
//!
//! Admission and close race safely without a lock: `admit` counts the work
//! before it reads `closed`, and `close` sets `closed` before anyone reads
//! the count, so either the admit sees the close and backs out, or the
//! closer's drain sees the admitted work.

use std::future::poll_fn;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use regolith::sync::{Event, Notify};

pub(crate) struct Gate {
    parent: Option<Arc<Gate>>,
    closed: AtomicBool,
    sealed: AtomicBool,
    in_flight: AtomicUsize,
    drained: Notify,
    closing: Event,
}

impl Gate {
    pub(crate) fn root() -> Arc<Self> {
        Arc::new(Self::under(None))
    }

    pub(crate) fn child(self: &Arc<Self>) -> Arc<Self> {
        Arc::new(Self::under(Some(Arc::clone(self))))
    }

    fn under(parent: Option<Arc<Gate>>) -> Self {
        Self {
            parent,
            closed: AtomicBool::new(false),
            sealed: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            drained: Notify::new(),
            closing: Event::new(),
        }
    }

    /// A permit for one piece of work, or `None` once this gate or one
    /// above it closed.
    pub(crate) fn admit(self: &Arc<Self>) -> Option<Permit> {
        let parent = match &self.parent {
            Some(parent) => Some(Box::new(parent.admit()?)),
            None => None,
        };
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        let permit = Permit {
            gate: Arc::clone(self),
            _parent: parent,
        };
        if self.closed.load(Ordering::SeqCst) {
            return None;
        }
        Some(permit)
    }

    /// A permit for work that continues a request already admitted: after
    /// the gate closes it is granted only while admitted work is still in
    /// flight, and never once the drain sealed the gate. `None` when it is
    /// refused.
    pub(crate) fn admit_continuation(self: &Arc<Self>) -> Option<Permit> {
        let parent = match &self.parent {
            Some(parent) => Some(Box::new(parent.admit_continuation()?)),
            None => None,
        };
        loop {
            let current = self.in_flight.load(Ordering::SeqCst);
            if current == 0 && self.closed.load(Ordering::SeqCst) {
                return None;
            }
            if self
                .in_flight
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                break;
            }
        }
        let permit = Permit {
            gate: Arc::clone(self),
            _parent: parent,
        };
        if self.sealed.load(Ordering::SeqCst) {
            return None;
        }
        Some(permit)
    }

    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.closing.set();
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.is_closed())
    }

    pub(crate) fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Completes once nothing this gate admitted is still running. Once
    /// closed, the first zero seals the gate and is looked at again, so a
    /// continuation counted in from zero either sees the seal or is seen
    /// here; both sides are SeqCst, which that store-then-load pairing needs.
    pub(crate) async fn drained(&self) {
        loop {
            let notified = self.drained.notified();
            if self.in_flight() == 0 {
                if !self.closed.load(Ordering::SeqCst) {
                    return;
                }
                self.sealed.store(true, Ordering::SeqCst);
                if self.in_flight() == 0 {
                    return;
                }
            }
            notified.await;
        }
    }

    /// Completes once this gate or one above it closes.
    pub(crate) async fn closing(self: Arc<Self>) {
        let chain: Vec<Arc<Gate>> =
            std::iter::successors(Some(self), |gate| gate.parent.clone()).collect();
        let mut waits: Vec<_> = chain.iter().map(|gate| gate.closing.wait()).collect();
        poll_fn(|cx| {
            if waits
                .iter_mut()
                .any(|wait| Pin::new(wait).poll(cx).is_ready())
            {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await
    }
}

/// One admitted piece of work. Dropping it, when the work ends however it
/// ends, releases the gate and every gate above it.
pub(crate) struct Permit {
    gate: Arc<Gate>,
    _parent: Option<Box<Permit>>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        if self.gate.in_flight.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.gate.drained.notify_waiters();
        }
    }
}
