//! A worker's own transactions: the mailbox of each, with the key of the
//! caller that owns it, by ID, and the sequence its IDs are drawn from. The
//! worker inserts and removes its own entries. The edge reads an entry to
//! turn a foreign caller away without a hop, and an abandon takes an entry
//! out at once (so no later request finds it) and queues the abandon for the
//! worker behind what is already there.
//!
//! The map drops a removed mailbox only when its reclamation runs, so nothing
//! here relies on a mailbox closing: the end of a transaction is always a
//! message.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use defra_core::thread_bounds::MaybeSend;
use rapidhash::fast::RandomState;
use regolith::sync::{ArrayQueue, HopscotchMap, UnboundedSender};

use super::actor::Message;
use super::TxnId;

/// One transaction's place in its owner's table.
pub(super) struct Entry<T, K>
where
    T: 'static,
{
    pub(super) mailbox: UnboundedSender<Message<T>>,
    /// The transaction while no request is using it, so an abandon drops it
    /// at once.
    pub(super) idle: Arc<ArrayQueue<T>>,
    /// Who may use the transaction.
    pub(super) key: K,
}

impl<T, K> Clone for Entry<T, K>
where
    T: 'static,
    K: Clone,
{
    fn clone(&self) -> Self {
        Self {
            mailbox: self.mailbox.clone(),
            idle: Arc::clone(&self.idle),
            key: self.key.clone(),
        }
    }
}

pub(super) struct Table<T, K>
where
    T: 'static,
    K: 'static,
{
    next: AtomicU64,
    entries: HopscotchMap<u64, Entry<T, K>, RandomState>,
}

impl<T, K> Table<T, K>
where
    T: MaybeSend + 'static,
    K: Clone + PartialEq + Send + 'static,
{
    pub(super) fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
            entries: HopscotchMap::with_hasher(RandomState::default()),
        }
    }

    /// The next sequence number for an ID.
    pub(super) fn draw(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    pub(super) fn insert(&self, id: TxnId, entry: Entry<T, K>) {
        self.entries.insert(id.get(), entry);
    }

    /// Whether transaction `id` is here and owned by `key`.
    pub(super) fn admits(&self, id: TxnId, key: &K) -> bool {
        self.entries
            .get(&id.get())
            .is_some_and(|entry| entry.key == *key)
    }

    /// Queues `message` for transaction `id`; hands it back when the
    /// transaction is not here.
    pub(super) fn send(&self, id: TxnId, message: Message<T>) -> Result<(), Message<T>> {
        match self.entries.get(&id.get()) {
            Some(entry) => entry.mailbox.try_send(message),
            None => Err(message),
        }
    }

    /// Queues one message, built per transaction, for every transaction here.
    pub(super) fn send_all(&self, mut message: impl FnMut() -> Message<T>) -> usize {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.mailbox.try_send(message()).is_ok())
            .count()
    }

    /// Takes transaction `id` out of the table. The first to take it ends
    /// it; every later request, and a racing end, finds none.
    pub(super) fn take(&self, id: TxnId) -> Option<Entry<T, K>> {
        self.entries.remove(&id.get())
    }

    /// Takes transaction `id` out of the table and lets it go: an idle
    /// transaction is dropped here and now, one a request is using is dropped
    /// by its task once that request is done.
    pub(super) fn abandon(&self, id: TxnId) {
        if let Some(entry) = self.take(id) {
            drop(entry.idle.pop());
            let _ = entry.mailbox.try_send(Message::Abandon);
        }
    }

    /// [`abandon`](Self::abandon) for every transaction here.
    pub(super) fn abandon_all(&self) {
        for id in self.entries.keys() {
            self.abandon(TxnId::from_raw(id));
        }
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}
