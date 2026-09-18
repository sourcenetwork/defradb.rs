//! Lossless invalidation of current document state, not a revision stream.

use std::sync::Arc;

use async_channel::{Receiver, Sender};
use kovan::Atom;
use rapidhash::RapidHashMap;

use crate::{TryRecvError, Update};

/// A document whose current state must be read again. No block payload is retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentChange {
    pub collection_id: String,
    pub doc_id: String,
    /// At least one coalesced update came from a local write.
    pub has_local_write: bool,
}

/// Atomically drained invalidations. Updates arriving during the subsequent read
/// remain pending for the next batch. This is not a snapshot or a durable cursor.
#[derive(Debug, Default)]
pub struct DocumentChangeBatch {
    pub changes: Vec<DocumentChange>,
    /// Too many distinct documents changed: reload the observer's entire scope.
    /// Never trust `changes` as a complete delta when this is true.
    pub resync_required: bool,
    pub updates: u64,
}

#[derive(Default, Clone)]
struct Pending {
    documents: RapidHashMap<(String, String), bool>,
    resync_required: bool,
    updates: u64,
}

#[derive(Clone)]
pub(crate) struct ChangePublisher {
    pending: Arc<Atom<Pending>>,
    wake: Sender<()>,
    capacity: usize,
}

impl ChangePublisher {
    pub(crate) fn publish(&self, update: &Update) {
        self.publish_batch(std::iter::once(update));
    }

    /// Coalesces `updates` into the shared `Pending` state in one RCU update.
    /// The whole apply-capacity-and-maybe-resync transition must commit
    /// atomically, so it lives in the `rcu` closure rather than split across
    /// independent primitives.
    pub(crate) fn publish_batch<'a>(&self, updates: impl Iterator<Item = &'a Update>) {
        let updates: Vec<&Update> = updates.collect();
        if updates.is_empty() {
            return;
        }
        let capacity = self.capacity;
        self.pending.rcu(|current| {
            let mut next = current.clone();
            for update in &updates {
                next.updates = next.updates.saturating_add(1);
                if !next.resync_required {
                    let key = (update.collection_id.clone(), update.doc_id.clone());
                    if let Some(local) = next.documents.get_mut(&key) {
                        *local |= !update.is_relay;
                    } else if next.documents.len() < capacity {
                        next.documents.insert(key, !update.is_relay);
                    } else {
                        next.documents.clear();
                        next.resync_required = true;
                    }
                }
            }
            next
        });
        // One pending wake is sufficient; the state above is authoritative.
        let _ = self.wake.try_send(());
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.wake.is_closed()
    }

    /// Explicitly closes the wake channel. Removal from the bus's observer
    /// map only drops a clone: the map's own reclamation keeps the original
    /// sender alive for a while longer, so a waiting `recv` would never see
    /// it close without this.
    pub(crate) fn close(&self) {
        self.wake.close();
    }
}

/// Bounded, coalescing subscription for consumers that query current state.
/// Repeated revisions of one document occupy one entry. Distinct-document
/// overflow becomes one explicit resync invalidation, never a silent loss.
/// Raw update/merge subscriptions and their CID/payload semantics are unchanged.
pub struct DocumentChangeSubscription {
    id: u64,
    pending: Arc<Atom<Pending>>,
    wake: Receiver<()>,
}

impl DocumentChangeSubscription {
    pub(crate) fn new(id: u64, capacity: usize) -> (ChangePublisher, Self) {
        let pending = Arc::new(Atom::new(Pending::default()));
        let (tx, rx) = async_channel::bounded(1);
        (
            ChangePublisher {
                pending: pending.clone(),
                wake: tx,
                capacity,
            },
            Self {
                id,
                pending,
                wake: rx,
            },
        )
    }

    pub(crate) fn closed() -> Self {
        let (_, subscription) = Self::new(0, 0);
        subscription
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub async fn recv(&mut self) -> Option<DocumentChangeBatch> {
        loop {
            self.wake.recv().await.ok()?;
            let batch = self.take_pending();
            // The wake and the pending update are two separate lock-free
            // writes (no single lock spans both), so a wake can occasionally
            // arrive just ahead of the update it announced: this drain then
            // races a concurrent one, takes nothing, and the announced data
            // lands right after. Waiting for the next wake recovers it.
            if batch.updates != 0 {
                return Some(batch);
            }
        }
    }

    pub fn try_recv(&mut self) -> Result<DocumentChangeBatch, TryRecvError> {
        loop {
            self.wake.try_recv().map_err(|error| match error {
                async_channel::TryRecvError::Empty => TryRecvError::Empty,
                async_channel::TryRecvError::Closed => TryRecvError::Disconnected,
            })?;
            let batch = self.take_pending();
            if batch.updates != 0 {
                return Ok(batch);
            }
        }
    }

    fn take_pending(&self) -> DocumentChangeBatch {
        let taken = self.pending.swap(Pending::default());
        // A publisher may have filled the wake slot between recv and this swap.
        // Its changes are included below, so consume that redundant wake too.
        let _ = self.wake.try_recv();
        DocumentChangeBatch {
            changes: taken
                .documents
                .clone()
                .into_iter()
                .map(
                    |((collection_id, doc_id), has_local_write)| DocumentChange {
                        collection_id,
                        doc_id,
                        has_local_write,
                    },
                )
                .collect(),
            resync_required: taken.resync_required,
            updates: taken.updates,
        }
    }
}
