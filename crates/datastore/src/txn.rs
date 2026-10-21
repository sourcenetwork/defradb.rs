use crate::error::{Error, Result};
use crate::multistore::{NamespaceView, RootView, SharedTxn};
/// Transaction wrapper for DefraDB matching Go's internal/datastore/txn.go.
///
/// BasicTxn wraps a corekv transaction and provides:
/// - Multistore access via namespace views
/// - Transaction ID
/// - Callbacks run once at its end (on_commit, on_abort)
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use storage::corekv::{
    CallbackMark, IterOptions, Iterator, OnAbort, OnCommit, Reader, RetryAttemptIO, Store, Txn,
    TxnCallbacks, Writer,
};

use storage::namespace::Namespace;

/// Transaction state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxnState {
    Active,
    Committed,
    Discarded,
}

/// BasicTxn wraps a corekv transaction with DefraDB-specific functionality.
///
/// This matches Go's BasicTxn in internal/datastore/txn.go, providing:
/// - Access to all namespaced stores (datastore, blockstore, etc.)
/// - Transaction ID for tracking
/// - Lifecycle callbacks
pub struct BasicTxn {
    shared_txn: Arc<SharedTxn>,
    id: u64,
    readonly: bool,
    state: TxnState,
    callbacks: TxnCallbacks,
    savepoints: Vec<CallbackMark>,
}

/// Internal Txn adapter used only to reuse the existing namespace view graph
/// inside an engine-owned retry. Its lifecycle methods never commit the
/// transaction; only `RetryTxn::finish_attempt_io` can return it to the engine.
struct AttemptScopedTxn {
    io: Option<Box<dyn RetryAttemptIO>>,
    callbacks: TxnCallbacks,
}

impl AttemptScopedTxn {
    fn new(io: Box<dyn RetryAttemptIO>) -> Self {
        Self {
            io: Some(io),
            callbacks: TxnCallbacks::default(),
        }
    }

    fn take(&mut self) -> storage::corekv::Result<(Box<dyn RetryAttemptIO>, TxnCallbacks)> {
        let io = self.io.take().ok_or(storage::corekv::Error::DiscardedTxn)?;
        Ok((io, std::mem::take(&mut self.callbacks)))
    }

    fn io(&self) -> storage::corekv::Result<&dyn RetryAttemptIO> {
        self.io
            .as_deref()
            .ok_or(storage::corekv::Error::DiscardedTxn)
    }
}

impl storage::corekv::private::Sealed for AttemptScopedTxn {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Reader for AttemptScopedTxn {
    async fn get(&self, key: &[u8]) -> storage::corekv::Result<Option<Bytes>> {
        self.io()?.get(key).await
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> storage::corekv::Result<Option<Bytes>> {
        self.io()?.get_parts(key, parts).await
    }

    async fn has(&self, key: &[u8]) -> storage::corekv::Result<bool> {
        self.io()?.has(key).await
    }

    async fn get_size(&self, key: &[u8]) -> storage::corekv::Result<Option<usize>> {
        self.io()?.get_size(key).await
    }

    async fn iterator(&self, options: IterOptions) -> storage::corekv::Result<Box<dyn Iterator>> {
        self.io()?.iterator(options).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Writer for AttemptScopedTxn {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> storage::corekv::Result<()> {
        self.io()?.merge_shared(key, operand).await
    }

    async fn append_shared(
        &self,
        log: &storage::corekv::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> storage::corekv::Result<()> {
        self.io()?.append_shared(log, entry, once_key).await
    }

    async fn truncate_log_shared(
        &self,
        log: &storage::corekv::AppendLog,
        positions: std::ops::RangeInclusive<u64>,
    ) -> storage::corekv::Result<()> {
        self.io()?.truncate_log_shared(log, positions).await
    }

    async fn appended(
        &self,
        log: &storage::corekv::AppendLog,
    ) -> storage::corekv::Result<Vec<Vec<u8>>> {
        self.io()?.appended(log).await
    }

    async fn set_shared(&self, key: &[u8], value: &[u8]) -> storage::corekv::Result<()> {
        self.io()?.set_shared(key, value).await
    }

    async fn delete_shared(&self, key: &[u8]) -> storage::corekv::Result<()> {
        self.io()?.delete_shared(key).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Txn for AttemptScopedTxn {
    async fn commit(self: Box<Self>) -> storage::corekv::Result<()> {
        Err(storage::corekv::Error::Other(
            "attempt-scoped transaction cannot commit independently".into(),
        ))
    }

    fn discard(self: Box<Self>) {}

    fn on_commit(&mut self, callback: OnCommit) -> storage::corekv::Result<()> {
        self.callbacks.on_commit(callback)
    }

    fn on_abort(&mut self, callback: OnAbort) {
        self.callbacks.on_abort(callback);
    }

    fn before_commit(&mut self, callback: storage::corekv::BeforeCommit) {
        self.callbacks.before_commit(callback);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn is_readonly(&self) -> bool {
        false
    }

    fn callback_count(&self) -> usize {
        self.callbacks.len()
    }
}

impl BasicTxn {
    /// Create a new BasicTxn from a store.
    pub async fn new<S: Store>(
        store: &S,
        id: u64,
        readonly: bool,
    ) -> storage::corekv::Result<Self> {
        let txn = store.new_txn(readonly).await?;
        Ok(Self::from_txn(txn, id, readonly))
    }

    /// Create a BasicTxn from an existing corekv transaction.
    pub fn from_txn(txn: Box<dyn Txn>, id: u64, readonly: bool) -> Self {
        Self {
            shared_txn: SharedTxn::new(txn),
            id,
            readonly,
            state: TxnState::Active,
            callbacks: TxnCallbacks::new(readonly),
            savepoints: Vec::new(),
        }
    }

    pub fn set_savepoint(&mut self) -> Result<()> {
        if self.state != TxnState::Active || self.readonly {
            return Err(Error::Storage(if self.readonly {
                storage::corekv::Error::ReadOnlyTxn
            } else {
                storage::corekv::Error::DiscardedTxn
            }));
        }
        self.alone()?.set_savepoint().map_err(Error::Storage)?;
        self.savepoints.push(self.callbacks.mark());
        Ok(())
    }

    pub fn rollback_to_savepoint(&mut self) -> Result<()> {
        let mark = *self.savepoints.last().ok_or_else(|| {
            Error::Storage(storage::corekv::Error::Other(
                "transaction has no active savepoint".into(),
            ))
        })?;
        self.alone()?
            .rollback_to_savepoint()
            .map_err(Error::Storage)?;
        self.savepoints.pop();
        self.callbacks.truncate(mark);
        Ok(())
    }

    pub fn release_savepoint(&mut self) -> Result<()> {
        if self.savepoints.is_empty() {
            return Err(Error::Storage(storage::corekv::Error::Other(
                "transaction has no active savepoint".into(),
            )));
        }
        self.alone()?.release_savepoint().map_err(Error::Storage)?;
        self.savepoints.pop();
        Ok(())
    }

    /// The transaction, held alone: a savepoint must not race a write, so
    /// every namespace view of it has to be gone. One still open is an error,
    /// never a wait.
    fn alone(&mut self) -> Result<&mut SharedTxn> {
        Arc::get_mut(&mut self.shared_txn).ok_or(Error::TxnStillInUse)
    }

    /// Create the namespace-capable facade for one engine-owned retry attempt.
    pub fn from_retry_attempt_io(io: Box<dyn RetryAttemptIO>, id: u64) -> Self {
        Self::from_txn(Box::new(AttemptScopedTxn::new(io)), id, false)
    }

    /// Drop all namespace views and recover the restricted IO capability plus
    /// callbacks for the store to attach to the winning commit.
    pub fn into_retry_attempt(
        self,
    ) -> storage::corekv::Result<(Box<dyn RetryAttemptIO>, TxnCallbacks)> {
        let Self {
            shared_txn,
            state,
            callbacks,
            ..
        } = self;
        if state != TxnState::Active {
            return Err(storage::corekv::Error::DiscardedTxn);
        }
        let mut txn = SharedTxn::try_into_txn(shared_txn).map_err(|_| {
            storage::corekv::Error::Other(
                "retry attempt retained namespace views or iterators".into(),
            )
        })?;
        let scoped = txn
            .as_any_mut()
            .downcast_mut::<AttemptScopedTxn>()
            .ok_or_else(|| {
                storage::corekv::Error::Other("transaction is not retry-scoped".into())
            })?;
        let (io, mut attempt_callbacks) = scoped.take()?;
        attempt_callbacks.append(callbacks);
        Ok((io, attempt_callbacks))
    }

    /// Get the transaction ID.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Check if this is a read-only transaction.
    pub fn is_readonly(&self) -> bool {
        self.readonly
    }

    /// Get the blockstore (namespace 'b').
    pub fn blockstore(&self) -> NamespaceView {
        NamespaceView::new(self.shared_txn.clone(), Namespace::Blockstore)
    }

    /// Get the datastore (namespace 'd').
    pub fn datastore(&self) -> NamespaceView {
        NamespaceView::new(self.shared_txn.clone(), Namespace::Datastore)
    }

    /// Get the encstore (namespace 'e').
    pub fn encstore(&self) -> NamespaceView {
        NamespaceView::new(self.shared_txn.clone(), Namespace::Encstore)
    }

    /// Get the headstore (namespace 'h').
    pub fn headstore(&self) -> NamespaceView {
        NamespaceView::new(self.shared_txn.clone(), Namespace::Headstore)
    }

    /// Get the peerstore (namespace 'p').
    pub fn peerstore(&self) -> NamespaceView {
        NamespaceView::new(self.shared_txn.clone(), Namespace::Peerstore)
    }

    /// Get the systemstore (namespace 's').
    pub fn systemstore(&self) -> NamespaceView {
        NamespaceView::new(self.shared_txn.clone(), Namespace::Systemstore)
    }

    /// Get the access-control policy store (namespace 'a').
    pub fn acpstore(&self) -> NamespaceView {
        NamespaceView::new(self.shared_txn.clone(), Namespace::Acpstore)
    }

    /// Get the rootstore (no namespace prefix).
    pub fn rootstore(&self) -> RootView {
        RootView::new(self.shared_txn.clone())
    }

    /// [`on_commit`](Self::on_commit) through a shared reference, for a
    /// transaction lent to the handles of one request. It joins the callbacks
    /// registered through views, in registration order, and reaches the
    /// transaction at its next savepoint, its commit or its end.
    pub fn on_commit_shared(&self, callback: OnCommit) -> Result<()> {
        self.shared_txn
            .queued()
            .on_commit(callback)
            .map_err(Error::Storage)
    }

    /// [`on_abort`](Self::on_abort) through a shared reference; see
    /// [`on_commit_shared`](Self::on_commit_shared).
    pub fn on_abort_shared(&self, callback: OnAbort) {
        self.shared_txn.queued().on_abort(callback);
    }

    /// [`before_commit`](Self::before_commit) through a shared reference; see
    /// [`on_commit_shared`](Self::on_commit_shared).
    pub fn before_commit_shared(&self, callback: storage::corekv::BeforeCommit) {
        self.shared_txn.queued().before_commit(callback);
    }

    /// Register `callback` to run once if the transaction commits. A
    /// read-only transaction refuses ordered work.
    pub fn on_commit(&mut self, callback: OnCommit) -> Result<()> {
        self.callbacks.on_commit(callback).map_err(Error::Storage)
    }

    /// Register `callback` to run once if the transaction ends without
    /// committing.
    pub fn on_abort(&mut self, callback: OnAbort) {
        self.callbacks.on_abort(callback);
    }

    /// Register `callback` to run once just before the commit is attempted.
    /// An error from it fails the commit, and the abort callbacks run.
    pub fn before_commit(&mut self, callback: storage::corekv::BeforeCommit) {
        self.callbacks.before_commit(callback);
    }

    /// Commit the transaction.
    ///
    /// On success the commit callbacks run; on error the abort callbacks are
    /// awaited.
    ///
    /// # Callback Panic Handling
    ///
    /// Callback panics are not recovered here. In dev/test profiles the panic
    /// unwinds through `commit()`. In release builds this workspace uses
    /// `panic = "abort"`, so a panicking callback aborts the process.
    pub async fn commit(mut self) -> Result<()> {
        if self.state != TxnState::Active {
            return Err(match self.state {
                TxnState::Committed => Error::TxnAlreadyCommitted,
                TxnState::Discarded => Error::TxnAlreadyDiscarded,
                TxnState::Active => unreachable!(),
            });
        }

        let mut callbacks = std::mem::take(&mut self.callbacks);
        if let Err(error) = callbacks.run_before_commit() {
            let (_, aborted) = callbacks.into_ends();
            aborted.run().await;
            return Err(Error::Storage(error));
        }
        let (committed, aborted) = callbacks.into_ends();
        let shared = Arc::try_unwrap(self.shared_txn).map_err(|_| {
            Error::Storage(storage::corekv::Error::Other(
                "Cannot commit: transaction still has references".into(),
            ))
        })?;

        // The work ordered by the commit receipt goes to the transaction that
        // gets the receipt; the rest runs here once the outcome is known.
        let mut txn = shared.into_txn();
        for event in committed.events {
            txn.on_commit(OnCommit::Ordered(event))
                .map_err(Error::Storage)?;
        }
        for hook in committed.hooks {
            txn.on_commit(OnCommit::OrderedAsync(hook))
                .map_err(Error::Storage)?;
        }
        let result = txn.commit().await;

        // Callbacks run directly so the runtime's panic strategy stays explicit:
        // unwind propagates in dev/test, and release aborts the process.
        if result.is_ok() {
            self.state = TxnState::Committed;
            committed.then.run().await;
        } else {
            aborted.run().await;
        }
        result.map_err(Error::Storage)
    }

    /// Discard the transaction.
    ///
    /// The abort callbacks run whenever the transaction was active. Returns an
    /// error if it was not, or if a namespace view still holds it: that view
    /// rolls the engine transaction back when it drops.
    ///
    /// # Callback Panic Handling
    ///
    /// Callback panics are not recovered here. In dev/test profiles the panic
    /// unwinds through `discard()`. In release builds this workspace uses
    /// `panic = "abort"`, so a panicking callback aborts the process.
    ///
    /// # Async Callback Warning
    ///
    /// Async abort callbacks are spawned as background tasks and may not complete
    /// if the process exits before they finish. For completion guarantees, use
    /// sync callbacks or prefer `commit()` when async cleanup is critical.
    pub fn discard(mut self) -> Result<()> {
        if self.state != TxnState::Active {
            return Err(match self.state {
                TxnState::Committed => Error::TxnAlreadyCommitted,
                TxnState::Discarded => Error::TxnAlreadyDiscarded,
                TxnState::Active => unreachable!(),
            });
        }

        let (_, aborted) = std::mem::take(&mut self.callbacks).into_ends();
        self.state = TxnState::Discarded;
        // The discard happens whatever else still holds the transaction: its
        // abort callbacks run now. A namespace view still open keeps the
        // engine transaction until the view drops, which rolls it back; that
        // is reported, not a reason to skip the rest.
        let released = match Arc::try_unwrap(self.shared_txn) {
            Ok(shared) => {
                shared.into_txn().discard();
                Ok(())
            }
            Err(_) => Err(Error::TxnStillInUse),
        };
        aborted.run_now();
        released
    }
}
