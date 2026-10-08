use async_lock::{Mutex as AsyncMutex, MutexGuardArc};
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use std::sync::Arc;

const PRUNE_THRESHOLD: usize = 10_000;

// DEFRALEVEL(S7): Delete the arrival sentence when the guard goes.
// DEFRALEVEL(S7): Delete the struct and its module once S3, S5 and S6 land.
/// Per-document write serialization queue.
///
/// Serializes mutations that touch the same document so that a local write and
/// a P2P merge (or two of either) never interleave their read-modify-write on a
/// document's CRDT state. This is required for counter convergence: a local
/// increment and an incoming merge both read-modify-write the counter
/// accumulation store, and without per-doc serialization their txns can race in
/// a way the underlying store's optimistic-conflict detection does not always
/// catch, dropping increments while the commit DAG still converges (#1021).
///
/// The merge handler shares the DB's instance of this queue (it already holds an
/// `Arc<DB>`), so local writes and merges contend on the same per-doc lock.
/// Arrival allocation also uses collection guards from this queue. Document
/// updates that do not allocate arrivals can still proceed independently.
pub struct DocWriteQueue {
    // DEFRALEVEL(S7): Delete once counters are blind merges (S3) and heads use markers (S4).
    locks: HopscotchMap<String, Arc<AsyncMutex<()>>, RandomState>,
    // DEFRALEVEL(S7): Delete together with per-doc locks.
    /// Serializes the guard-ACQUISITION phase of multi-document writers (local
    /// mutation batches, batch merges) against one another. A caller that will
    /// hold more than one per-doc guard at once must hold this gate while
    /// acquiring them, so two multi-doc acquirers can never grab overlapping
    /// documents in opposite orders and deadlock. Single-doc callers never take
    /// it, so the common path (single-doc writes and merges) is unaffected.
    ///
    /// Deadlock-freedom also relies on `new_txn()` being non-blocking (the
    /// backends use optimistic MVCC: a write txn snapshots on open and acquires
    /// the exclusive store write lock only at commit, which holds neither the gate
    /// nor any per-doc guard). That keeps the gate the only resource ever
    /// contended across a txn open, so the two acquirer orderings (BatchMutator
    /// opens its txn before taking the gate; create_many / try_batch_merge take
    /// the gate before opening their txn) cannot invert into a cycle. A future
    /// blocking-writer backend would need to revisit this.
    batch_gate: Arc<AsyncMutex<()>>,
}

impl Default for DocWriteQueue {
    fn default() -> Self {
        Self {
            locks: HopscotchMap::with_hasher(RandomState::default()),
            batch_gate: Arc::new(AsyncMutex::new(())),
        }
    }
}

impl DocWriteQueue {
    pub fn new() -> Self {
        Self::default()
    }

    // DEFRALEVEL(S7): Removed; conflicts surface instead as regolith DefraLevel commit conflicts.
    /// Acquire the write lock for a document.
    ///
    /// Returns an owned guard that serializes access. Different documents
    /// proceed in parallel; the same document blocks until the previous holder
    /// drops the guard.
    pub async fn acquire(&self, doc_id: &str) -> MutexGuardArc<()> {
        loop {
            let mutex = self.mutex_for(doc_id);
            let guard = mutex.lock_arc().await;
            // A prune that dropped this entry between the lookup and the lock
            // would let the next acquirer create a second mutex for the same
            // document, so no work happens under an unpublished guard: release
            // it and take the entry the map holds now.
            if self
                .locks
                .get(doc_id)
                .is_some_and(|current| Arc::ptr_eq(&current, &mutex))
            {
                return guard;
            }
        }
    }

    fn mutex_for(&self, doc_id: &str) -> Arc<AsyncMutex<()>> {
        if let Some(mutex) = self.locks.get(doc_id) {
            return mutex;
        }
        if self.locks.len() > PRUNE_THRESHOLD {
            self.prune();
        }
        self.locks
            .get_or_insert(doc_id.to_string(), Arc::new(AsyncMutex::new(())))
    }

    /// Drop the entries no acquirer references. The map holds one reference and
    /// the iterator's clone a second; a third means an acquirer holds the guard
    /// or is taking it, and that entry stays.
    fn prune(&self) {
        for (doc_id, mutex) in self.locks.iter() {
            if Arc::strong_count(&mutex) <= 2 {
                self.locks.remove(&doc_id);
            }
        }
    }

    // DEFRALEVEL(S7,S6): Delete it; with S6 there is no shared RMW key left to serialize.
    /// Serialize arrival allocation before opening a transaction. The `arrival:`
    /// prefix is disjoint from content-addressed document IDs. Acquire collection
    /// guards first, then arrival guards in sorted collection-ID order, then any
    /// document guards. Explicit transactions retain optimistic conflict handling
    /// and never acquire this guard after opening their snapshot.
    pub(crate) async fn acquire_arrival(&self, collection_id: &str) -> MutexGuardArc<()> {
        self.acquire(&format!("arrival:{collection_id}")).await
    }

    // DEFRALEVEL(S7): Removed together with the per-doc guards.
    /// Acquire the multi-document batch gate.
    ///
    /// Any caller that will simultaneously hold more than one per-doc guard must
    /// hold this gate while acquiring those guards. An incremental acquirer (a
    /// local mutation batch that discovers its documents one mutation at a time)
    /// holds it for the whole batch; an upfront acquirer (a batch merge, or
    /// `create_many`) holds it only while taking its sorted guards, then releases
    /// it. This makes the per-doc guards deadlock-free across multi-doc writers.
    pub async fn acquire_batch_gate(&self) -> MutexGuardArc<()> {
        self.batch_gate.lock_arc().await
    }

    // DEFRALEVEL(S7): Removed; batch merge no longer needs a gate and MergeError::GateContended.
    /// Non-blocking variant of [`Self::acquire_batch_gate`]. Returns `None` if the
    /// gate is currently held. A caller for whom batching is an optimization (the
    /// batch-merge path) uses this to degrade to the gate-free per-block path
    /// instead of blocking behind a long-lived gate holder (e.g. an interactive
    /// transaction that holds the gate across its user-controlled lifetime, #1041).
    pub fn try_acquire_batch_gate(&self) -> Option<MutexGuardArc<()>> {
        self.batch_gate.try_lock_arc()
    }
}
