//! Lossless invalidation of current document state, not a revision stream.

use std::collections::HashMap;
use std::sync::Arc;

use async_channel::{Receiver, Sender};
use parking_lot::Mutex;

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

#[derive(Default)]
struct Pending {
    documents: HashMap<(String, String), bool>,
    resync_required: bool,
    updates: u64,
}

pub(crate) struct ChangePublisher {
    pending: Arc<Mutex<Pending>>,
    wake: Sender<()>,
    capacity: usize,
}

impl ChangePublisher {
    pub(crate) fn publish(&self, update: &Update) {
        let mut pending = self.pending.lock();
        pending.updates = pending.updates.saturating_add(1);
        if !pending.resync_required {
            let key = (update.collection_id.clone(), update.doc_id.clone());
            if let Some(local) = pending.documents.get_mut(&key) {
                *local |= !update.is_relay;
            } else if pending.documents.len() < self.capacity {
                pending.documents.insert(key, !update.is_relay);
            } else {
                pending.documents.clear();
                pending.resync_required = true;
            }
        }
        // One pending wake is sufficient; the state above is authoritative.
        let _ = self.wake.try_send(());
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.wake.is_closed()
    }
}

/// Bounded, coalescing subscription for consumers that query current state.
/// Repeated revisions of one document occupy one entry. Distinct-document
/// overflow becomes one explicit resync invalidation, never a silent loss.
/// Raw update/merge subscriptions and their CID/payload semantics are unchanged.
pub struct DocumentChangeSubscription {
    id: u64,
    pending: Arc<Mutex<Pending>>,
    wake: Receiver<()>,
}

impl DocumentChangeSubscription {
    pub(crate) fn new(id: u64, capacity: usize) -> (ChangePublisher, Self) {
        let pending = Arc::new(Mutex::new(Pending::default()));
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
        self.wake.recv().await.ok()?;
        Some(self.take_pending())
    }

    pub fn try_recv(&mut self) -> Result<DocumentChangeBatch, TryRecvError> {
        self.wake.try_recv().map_err(|error| match error {
            async_channel::TryRecvError::Empty => TryRecvError::Empty,
            async_channel::TryRecvError::Closed => TryRecvError::Disconnected,
        })?;
        Ok(self.take_pending())
    }

    fn take_pending(&self) -> DocumentChangeBatch {
        let mut pending = self.pending.lock();
        // A publisher may have filled the wake slot between recv and this lock.
        // Its changes are included below, so consume that redundant wake too.
        let _ = self.wake.try_recv();
        let taken = std::mem::take(&mut *pending);
        drop(pending);
        DocumentChangeBatch {
            changes: taken
                .documents
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
