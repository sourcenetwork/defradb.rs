//! A regolith-backed transaction.
//!
//! regolith owns the write buffer, read set, and commit validation. A bounded
//! native commit publishes writes without an adapter publication lock. Head
//! rows and supersession markers are read through engine snapshots.
//!
//! A read-only transaction does not begin one at all: it pins a snapshot,
//! which is the same view without the commit-time validation nothing will
//! ever ask for.

#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use regolith::{
    IsolationLevel, OptimisticTransactionDb, Transaction, TransactionError, TxnOptions,
};

use super::event_order::EventOrder;
use super::handle::Handle;
use super::iterator::RegolithIterator;
use super::post_commit::PostCommit;
use super::slots::TxnSlot;
use super::worker_io::{readied, settled};
use crate::corekv::{
    CallbackMark, Error, IterOptions, Iterator, OnAbort, OnCommit, Reader, Result, Txn,
    TxnCallbacks, Writer,
};

/// Transaction over a regolith store.
pub struct RegolithTxn {
    handle: Option<Arc<Handle>>,
    /// This transaction's place in its store's in-flight count, held until
    /// the transaction and its post-commit work are done. A read-only open
    /// and a retry attempt's view count against no store.
    _slot: Option<TxnSlot>,
    callbacks: TxnCallbacks,
    readonly: bool,
    event_order: Arc<EventOrder>,
    savepoints: Vec<CallbackMark>,
    scoped_attempt: bool,
}

impl RegolithTxn {
    /// Open a snapshot of an existing on-disk database without modifying files,
    /// with the engine options, at-rest keys included, a writable store of it
    /// uses. This never rewrites a legacy store.
    pub fn open_read_only(
        path: impl AsRef<std::path::Path>,
        options: super::RegolithStoreOptions,
    ) -> Result<Self> {
        let db = regolith::Db::open_read_only(path.as_ref(), options.engine_for_open()).map_err(
            |error| Error::Backend(format!("failed to open regolith read-only: {error}")),
        )?;
        #[cfg(not(target_arch = "wasm32"))]
        super::rewrite::check_read_only(path.as_ref(), &db, &options)?;
        Ok(Self {
            handle: Some(Arc::new(Handle::ReadOnly(db.snapshot()))),
            _slot: None,
            callbacks: TxnCallbacks::new(true),
            readonly: true,
            savepoints: Vec::new(),
            event_order: Arc::new(EventOrder::default()),
            scoped_attempt: false,
        })
    }

    pub(super) fn new(
        db: &Arc<OptimisticTransactionDb>,
        readonly: bool,
        isolation: IsolationLevel,
        slot: TxnSlot,
        event_order: Arc<EventOrder>,
        mode: regolith::ReadMode,
    ) -> Result<Self> {
        // On a pool the handle reads on a worker's queue, so a cache miss
        // suspends the task instead of the thread.
        let handle = if readonly {
            Handle::ReadOnly(db.db().snapshot().with_read_mode(mode))
        } else {
            let options = TxnOptions::new().isolation(isolation).read_mode(mode);
            Handle::Writable(Box::new(db.begin(&options)))
        };
        Ok(Self {
            handle: Some(Arc::new(handle)),
            _slot: Some(slot),
            callbacks: TxnCallbacks::new(readonly),
            readonly,
            savepoints: Vec::new(),
            event_order,
            scoped_attempt: false,
        })
    }

    pub(super) fn scoped_attempt(txn: Box<Transaction>) -> Self {
        Self {
            handle: Some(Arc::new(Handle::Writable(txn))),
            _slot: None,
            callbacks: TxnCallbacks::default(),
            readonly: false,
            event_order: Arc::new(EventOrder::default()),
            savepoints: Vec::new(),
            scoped_attempt: true,
        }
    }

    // Ownership arrives boxed from the sealed attempt IO capability.
    #[allow(clippy::boxed_local)]
    pub(super) fn into_scoped_engine_txn(mut self: Box<Self>) -> Result<Box<Transaction>> {
        if !self.scoped_attempt {
            return Err(Error::Other("not a scoped retry transaction".into()));
        }
        let handle = self.handle.take().ok_or(Error::DiscardedTxn)?;
        let handle = Arc::try_unwrap(handle).map_err(|_| {
            Error::Other("retry transaction still has retained views or iterators".into())
        })?;
        let Handle::Writable(txn) = handle else {
            return Err(Error::ReadOnlyTxn);
        };
        Ok(txn)
    }

    /// Ends without a commit, where nothing can await: the abort callbacks
    /// run now, their async ones spawned.
    fn abort_now(&mut self) {
        // Most transactions register nothing: they skip sorting and dropping
        // empty ends.
        if self.callbacks.is_empty() {
            return;
        }
        let (_, aborted) = std::mem::take(&mut self.callbacks).into_ends();
        aborted.run_now();
    }

    fn handle(&self) -> Result<&Arc<Handle>> {
        self.handle.as_ref().ok_or(Error::DiscardedTxn)
    }

    fn writable(&self) -> Result<&Transaction> {
        match self.handle()?.as_ref() {
            Handle::Writable(txn) => Ok(txn),
            Handle::ReadOnly(_) => Err(Error::ReadOnlyTxn),
        }
    }

    fn writable_mut(&mut self) -> Result<&mut Transaction> {
        let handle = self.handle.as_mut().ok_or(Error::DiscardedTxn)?;
        let handle = Arc::get_mut(handle).ok_or_else(|| {
            Error::Other("cannot savepoint while transaction iterators are active".into())
        })?;
        match handle {
            Handle::Writable(txn) => Ok(txn),
            Handle::ReadOnly(_) => Err(Error::ReadOnlyTxn),
        }
    }
}

impl Drop for RegolithTxn {
    /// A transaction dropped without a commit or a discard ended without
    /// committing: regolith rolls its writes back, and its discard
    /// callbacks run here, once. Not while unwinding, where a callback that
    /// panicked too would abort the process.
    fn drop(&mut self) {
        if self.handle.take().is_some() && !std::thread::panicking() {
            self.abort_now();
        }
    }
}

pub(super) fn map_txn_error(error: TransactionError) -> Error {
    match error {
        TransactionError::Engine(error) => error.into(),
        TransactionError::Conflict(conflict) => {
            Error::TxnConflictDetails(crate::corekv::TransactionConflict::from(&conflict))
        }
        // Lock admission failure is not an optimistic validation conflict.
        // Keep it transient and typed as a backend failure so outer callers
        // do not count it as another storage-conflict retry.
        busy @ TransactionError::Busy(_) => Error::Backend(busy.to_string()),
        other => Error::Backend(other.to_string()),
    }
}

impl crate::corekv::private::Sealed for RegolithTxn {}

impl crate::corekv::RetryAttemptIO for RegolithTxn {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Reader for RegolithTxn {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        // `get_slice` borrows the bytes the engine already holds and
        // `Bytes::from_owner` adopts that borrow, so a read copies
        // nothing on the way to the caller.
        let slice = match self.handle()?.as_ref() {
            Handle::ReadOnly(snapshot) => settled!(snapshot.get_slice(key)).map_err(Error::from)?,
            Handle::Writable(txn) => readied!(txn.get_slice(key)).map_err(map_txn_error)?,
        };
        Ok(slice.map(Bytes::from_owner))
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<Bytes>> {
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        let slice = match self.handle()?.as_ref() {
            Handle::ReadOnly(snapshot) => settled!(snapshot.get_slice(key)).map_err(Error::from)?,
            Handle::Writable(txn) => readied!(txn.get_parts(key, parts)).map_err(map_txn_error)?,
        };
        Ok(slice.map(Bytes::from_owner))
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        match self.handle()?.as_ref() {
            Handle::ReadOnly(snapshot) => settled!(snapshot.has(key)).map_err(Error::from),
            // A writing transaction has to consult its own buffer, which
            // `has` on the snapshot underneath would miss.
            Handle::Writable(txn) => Ok(readied!(txn.get_slice(key))
                .map_err(map_txn_error)?
                .is_some()),
        }
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        match self.handle()?.as_ref() {
            Handle::ReadOnly(snapshot) => settled!(snapshot.get_size(key)).map_err(Error::from),
            Handle::Writable(txn) => Ok(readied!(txn.get_slice(key))
                .map_err(map_txn_error)?
                .map(|v| v.len())),
        }
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        // The iterator holds the same handle, so it keeps the snapshot or
        // the transaction alive without borrowing this one. Scanning a
        // transaction that is still being written to is the expensive
        // shape, because the merged stream is rebuilt per page: open a
        // read-only transaction where the scan does not need to see
        // uncommitted writes.
        let handle = Arc::clone(self.handle()?);
        Ok(Box::new(RegolithIterator::open(handle, &opts)?))
    }
}

impl RegolithTxn {
    /// The transaction a write of `key` goes to: refused for an empty key or
    /// a read-only transaction. Both forms of each `Writer` method call it
    /// and then make the write in place, so a write boxes the one future its
    /// trait method returns and waits only when the engine asks it to.
    fn write_target(&self, key: &[u8]) -> Result<&Transaction> {
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        self.writable()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Writer for RegolithTxn {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        let txn = self.write_target(key)?;
        readied!(txn.merge(key, operand)).map_err(map_txn_error)
    }

    async fn append_shared(
        &self,
        log: &crate::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        let layout: Arc<dyn regolith::LogLayout> = log.keys().clone();
        let txn = self.writable()?;
        readied!(txn.append(&layout, entry, once_key)).map_err(map_txn_error)
    }

    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let txn = self.write_target(key)?;
        readied!(txn.put(key, value)).map_err(map_txn_error)
    }

    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        let txn = self.write_target(key)?;
        readied!(txn.delete(key)).map_err(map_txn_error)
    }

    async fn merge(&mut self, key: &[u8], operand: &[u8]) -> Result<()> {
        let txn = self.write_target(key)?;
        readied!(txn.merge(key, operand)).map_err(map_txn_error)
    }

    async fn append(
        &mut self,
        log: &crate::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        let layout: Arc<dyn regolith::LogLayout> = log.keys().clone();
        let txn = self.writable()?;
        readied!(txn.append(&layout, entry, once_key)).map_err(map_txn_error)
    }

    async fn set(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        let txn = self.write_target(key)?;
        readied!(txn.put(key, value)).map_err(map_txn_error)
    }

    async fn delete(&mut self, key: &[u8]) -> Result<()> {
        let txn = self.write_target(key)?;
        readied!(txn.delete(key)).map_err(map_txn_error)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Txn for RegolithTxn {
    async fn commit(mut self: Box<Self>) -> Result<()> {
        if self.scoped_attempt {
            return Err(Error::Other(
                "scoped retry transaction cannot commit independently".into(),
            ));
        }
        let handle = self.handle.take().ok_or(Error::DiscardedTxn)?;
        let mut callbacks = std::mem::take(&mut self.callbacks);
        if let Err(error) = callbacks.run_before_commit() {
            let (_, aborted) = callbacks.into_ends();
            aborted.run().await;
            return Err(error);
        }
        // regolith's `commit` consumes the transaction, so this has to be
        // its only owner. An iterator opened from it holds the same `Arc`,
        // so a caller that commits without closing one gets told, rather
        // than a commit that silently does not happen.
        let Ok(handle) = Arc::try_unwrap(handle) else {
            let (_, aborted) = callbacks.into_ends();
            aborted.run().await;
            return Err(Error::Other(
                "cannot commit while an iterator opened from this transaction is still open"
                    .to_string(),
            ));
        };
        let Handle::Writable(mut txn) = handle else {
            // A snapshot has nothing to commit and cannot carry ordered work.
            let (committed, _) = callbacks.into_ends();
            committed.then.run().await;
            return Ok(());
        };
        let work = PostCommit::bind(&self.event_order, &mut txn, callbacks);
        // A transaction on a worker's queue commits without waiting: the
        // task awaits the ticket the worker's own queue completes. Anywhere
        // else the commit syncs on the calling thread.
        let outcome = if txn.io_queue().is_some() {
            txn.commit_nowait().await
        } else {
            txn.commit()
        };
        match outcome.map_err(map_txn_error) {
            // Durable from here on: what follows can be reported, never
            // turned into a failed write.
            Ok(_) => {
                work.committed().await;
                Ok(())
            }
            Err(error) => {
                work.failed().await;
                Err(error)
            }
        }
    }

    fn set_savepoint(&mut self) -> Result<()> {
        let mark = self.callbacks.mark();
        self.writable_mut()?.set_savepoint();
        self.savepoints.push(mark);
        Ok(())
    }

    fn rollback_to_savepoint(&mut self) -> Result<()> {
        let mark = *self
            .savepoints
            .last()
            .ok_or_else(|| Error::Other("transaction has no active savepoint".into()))?;
        self.writable_mut()?
            .rollback_to_savepoint()
            .map_err(map_txn_error)?;
        self.savepoints.pop();
        self.callbacks.truncate(mark);
        Ok(())
    }

    fn release_savepoint(&mut self) -> Result<()> {
        self.savepoints
            .last()
            .ok_or_else(|| Error::Other("transaction has no active savepoint".into()))?;
        self.writable_mut()?
            .release_savepoint()
            .map_err(map_txn_error)?;
        self.savepoints.pop();
        Ok(())
    }

    fn discard(mut self: Box<Self>) {
        // An iterator still open keeps the transaction alive; regolith
        // rolls it back when that last handle drops.
        if let Some(Ok(Handle::Writable(txn))) = self.handle.take().map(Arc::try_unwrap) {
            txn.rollback();
        }
        self.abort_now();
    }

    fn on_commit(&mut self, callback: OnCommit) -> Result<()> {
        self.callbacks.on_commit(callback)
    }

    fn on_abort(&mut self, callback: OnAbort) {
        self.callbacks.on_abort(callback);
    }

    fn before_commit(&mut self, callback: crate::corekv::BeforeCommit) {
        self.callbacks.before_commit(callback);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn is_readonly(&self) -> bool {
        self.readonly
    }

    fn callback_count(&self) -> usize {
        self.callbacks.len()
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn transaction_mapping_retains_io_facts_without_making_them_retryable() {
        let os = std::io::Error::from_raw_os_error(28);
        let kind = os.kind();
        let message = os.to_string();
        let error = map_txn_error(TransactionError::Engine(regolith::Error::Io(os)));
        assert!(!error.is_retriable());
        let Error::Io(cause) = error else {
            panic!("I/O cause lost")
        };
        assert_eq!(cause.kind(), kind);
        assert_eq!(cause.raw_os_error(), Some(28));
        assert_eq!(cause.to_string(), message);
        assert_eq!(cause.clone(), cause);
    }

    #[test]
    fn corruption_with_disk_full_text_is_not_an_io_failure() {
        let error = map_txn_error(TransactionError::Engine(regolith::Error::Corruption(
            std::io::Error::new(std::io::ErrorKind::StorageFull, "full"),
        )));
        assert!(matches!(error, Error::Backend(_)));
        assert!(!error.is_retriable());
    }
}

#[cfg(test)]
mod append_tests {
    use crate::backends::RegolithStore;
    use crate::corekv::{AppendLog, Store, Writer};

    /// A pending append holds the layout its log already shares, not a copy.
    #[test]
    fn an_append_shares_its_log_layout() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let log = AppendLog::new(b"s/arrival/c1/head".to_vec(), b"s/arrival/c1/row/".to_vec());
            let mut txn = store.new_txn(false).await.unwrap();
            txn.append(&log, b"doc-1", Some(b"s/arrival/c1/doc/doc-1"))
                .await
                .unwrap();
            assert_eq!(std::sync::Arc::strong_count(log.keys()), 2);
            txn.discard();
        })
        .unwrap()
    }
}

#[cfg(test)]
mod callback_tests {
    use super::*;
    use crate::backends::RegolithStore;
    use crate::corekv::{Reader, Store, TxnCallback, Writer};
    use regolith::sync::SegQueue;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn counting(count: &Arc<AtomicUsize>) -> TxnCallback {
        let count = Arc::clone(count);
        Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        })
    }

    /// A before-commit error fails the commit: nothing is written, the abort
    /// callbacks run, and the commit callbacks do not.
    #[test]
    fn a_failing_before_commit_fails_the_commit_and_runs_the_abort_callbacks() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let committed = Arc::new(AtomicUsize::new(0));
            let aborted = Arc::new(AtomicUsize::new(0));
            let mut txn = store.new_txn(false).await.unwrap();
            txn.set(b"key", b"value").await.unwrap();
            txn.on_commit(OnCommit::Sync(counting(&committed))).unwrap();
            txn.on_abort(OnAbort::Sync(counting(&aborted)));
            txn.before_commit(Box::new(|| Err(Error::Other("not yet".into()))));
            let error = txn.commit().await.unwrap_err();
            assert!(error.to_string().contains("not yet"), "{error}");
            assert_eq!(committed.load(Ordering::SeqCst), 0);
            assert_eq!(aborted.load(Ordering::SeqCst), 1);
            let reader = store.new_txn(true).await.unwrap();
            assert!(reader.get(b"key").await.unwrap().is_none());
        })
        .unwrap()
    }

    /// A transaction dropped without a commit or a discard ends without
    /// committing, so its abort callbacks run, once.
    #[test]
    fn a_dropped_transaction_runs_its_abort_callbacks_once() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let aborted = Arc::new(AtomicUsize::new(0));
            let mut dropped = store.new_txn(false).await.unwrap();
            dropped.set(b"key", b"value").await.unwrap();
            dropped.on_abort(OnAbort::Sync(counting(&aborted)));
            drop(dropped);
            assert_eq!(aborted.load(Ordering::SeqCst), 1);

            let mut explicit = store.new_txn(false).await.unwrap();
            explicit.on_abort(OnAbort::Sync(counting(&aborted)));
            explicit.discard();
            assert_eq!(aborted.load(Ordering::SeqCst), 2);

            let mut committed = store.new_txn(false).await.unwrap();
            committed.set(b"key", b"value").await.unwrap();
            committed.on_abort(OnAbort::Sync(counting(&aborted)));
            committed.commit().await.unwrap();
            assert_eq!(aborted.load(Ordering::SeqCst), 2);
        })
        .unwrap()
    }

    /// A commit that loses a conflict runs its abort callbacks once and none
    /// of its commit callbacks, ordered ones included.
    #[test]
    fn a_commit_that_loses_runs_only_its_abort_callbacks() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            let mut seed = store.new_txn(false).await.unwrap();
            seed.set(b"key", b"one").await.unwrap();
            seed.commit().await.unwrap();

            let (committed, aborted) =
                (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
            let mut loser = store.new_txn(false).await.unwrap();
            assert!(loser.get(b"key").await.unwrap().is_some());
            loser.set(b"key", b"loser").await.unwrap();
            loser
                .on_commit(OnCommit::Sync(counting(&committed)))
                .unwrap();
            loser
                .on_commit(OnCommit::Ordered(counting(&committed)))
                .unwrap();
            loser.on_abort(OnAbort::Sync(counting(&aborted)));
            let aborted_async = Arc::clone(&aborted);
            loser.on_abort(OnAbort::Async(Box::new(move || {
                Box::pin(async move {
                    aborted_async.fetch_add(1, Ordering::SeqCst);
                })
            })));
            let mut winner = store.new_txn(false).await.unwrap();
            winner.set(b"key", b"winner").await.unwrap();
            winner.commit().await.unwrap();

            assert!(loser.commit().await.is_err());
            assert_eq!(committed.load(Ordering::SeqCst), 0);
            assert_eq!(aborted.load(Ordering::SeqCst), 2);
        })
        .unwrap()
    }

    /// After a commit the async callbacks are awaited before the sync ones
    /// run, each kind in registration order; a read-only transaction runs
    /// them the same way and refuses ordered work.
    #[test]
    fn a_commit_awaits_its_async_callbacks_before_its_sync_ones() {
        workers::block_on(async {
            let store = RegolithStore::in_memory().unwrap();
            for readonly in [false, true] {
                let seen = Arc::new(SegQueue::new());
                let mut txn = store.new_txn(readonly).await.unwrap();
                for name in ["sync-1", "async-1", "sync-2", "async-2"] {
                    let seen = Arc::clone(&seen);
                    let callback = if name.starts_with("async") {
                        OnCommit::Async(Box::new(move || Box::pin(async move { seen.push(name) })))
                    } else {
                        OnCommit::Sync(Box::new(move || seen.push(name)))
                    };
                    txn.on_commit(callback).unwrap();
                }
                assert_eq!(
                    txn.on_commit(OnCommit::Ordered(Box::new(|| {}))).is_err(),
                    readonly
                );
                txn.commit().await.unwrap();
                let order: Vec<&str> = std::iter::from_fn(|| seen.pop()).collect();
                assert_eq!(order, ["async-1", "async-2", "sync-1", "sync-2"]);
            }
        })
        .unwrap()
    }
}
