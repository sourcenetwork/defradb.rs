//! The hosted node, shared by the calls its host makes on it at once.
//!
//! Most calls read the node: a status, an export, a handover that ends at
//! the node's own deadline. A switch and a stop replace or remove it, and
//! wait until no call reads it. Two calls read it for as long as they wait,
//! possibly for ever: the watches for a demotion and a contest. A watch
//! therefore lets go whenever a switch or a stop asks for the node, and
//! watches the node that is there afterwards.
//!
//! Modeled by proofs/tla/EmbeddedVeraSwitch.tla.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::future::{select, Either};
use regolith::sync::{Notify, RwLock, RwLockReadGuard, RwLockWriteGuard};
use vera_node::EmbeddedNode;

use super::HostDevice;

/// A watch on one node: a future that borrows it. The node's own watches
/// are not `Send`, so neither is this.
pub(super) type Watch<'node, T> = Pin<Box<dyn Future<Output = T> + 'node>>;

/// What the slot holds: the node, `None` once stopped, and the device it
/// runs as. A switch that left no node behind records there the device it
/// was heading to, which [`HostedNode::complete_switch`](super::HostedNode::complete_switch)
/// starts.
pub(super) struct Slot {
    pub(super) node: Option<EmbeddedNode>,
    pub(super) device: HostDevice,
}

pub(super) struct Held {
    node: RwLock<Slot>,
    /// Switches and stops that asked for the node and have not let go of it.
    replacing: AtomicUsize,
    /// Told when a switch or a stop asks for the node.
    asked: Notify,
    /// Told when a switch or a stop has let go of it.
    released: Notify,
}

impl std::fmt::Debug for Held {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Held")
            .field("replacing", &self.replacing.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Held {
    pub(super) fn new(node: EmbeddedNode, device: HostDevice) -> Self {
        Self {
            node: RwLock::new(Slot {
                node: Some(node),
                device,
            }),
            replacing: AtomicUsize::new(0),
            asked: Notify::new(),
            released: Notify::new(),
        }
    }

    /// The node for a call that ends by itself. `None` inside once stopped.
    pub(super) async fn read(&self) -> RwLockReadGuard<'_, Slot> {
        self.node.read().await
    }

    /// The node for a switch or a stop, once no call reads it. Every watch
    /// lets go of it first. A call that gives up waiting withdraws its ask.
    pub(super) async fn replace(&self) -> Replacing<'_> {
        let asking = Asking::new(self);
        let guard = self.node.write().await;
        Replacing {
            guard,
            _asking: asking,
        }
    }

    /// What `watch` resolves to on the node that runs, across switches.
    /// `None` once the node is stopped.
    pub(super) async fn watch<T, W>(&self, watch: W) -> Option<T>
    where
        W: for<'node> Fn(&'node EmbeddedNode) -> Watch<'node, T>,
    {
        loop {
            // Both notifications reach a future created before they are sent.
            let released = self.released.notified();
            let asked = self.asked.notified();
            if self.replacing.load(Ordering::Acquire) > 0 {
                released.await;
                continue;
            }
            let guard = self.node.read().await;
            if self.replacing.load(Ordering::Acquire) > 0 {
                drop(guard);
                released.await;
                continue;
            }
            let watching = watch(guard.node.as_ref()?);
            futures::pin_mut!(asked);
            let outcome = select(watching, asked).await;
            if let Either::Left((value, _)) = outcome {
                return Some(value);
            }
        }
    }
}

/// A switch or a stop that asked for the node, until it lets go of it or
/// gives up waiting for it.
struct Asking<'held>(&'held Held);

impl<'held> Asking<'held> {
    fn new(held: &'held Held) -> Self {
        held.replacing.fetch_add(1, Ordering::AcqRel);
        held.asked.notify_waiters();
        Self(held)
    }
}

impl Drop for Asking<'_> {
    fn drop(&mut self) {
        self.0.replacing.fetch_sub(1, Ordering::AcqRel);
        self.0.released.notify_waiters();
    }
}

/// The node, held by a switch or a stop. The node is let go of before the
/// ask is withdrawn, so the watches it wakes find the node free: fields drop
/// in their order.
pub(super) struct Replacing<'held> {
    guard: RwLockWriteGuard<'held, Slot>,
    _asking: Asking<'held>,
}

impl std::ops::Deref for Replacing<'_> {
    type Target = Slot;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl std::ops::DerefMut for Replacing<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}
