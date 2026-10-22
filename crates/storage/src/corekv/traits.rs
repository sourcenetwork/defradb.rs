use async_trait::async_trait;
/// Core KV trait definitions matching the Go corekv package.
///
/// This module defines the complete trait hierarchy for key-value storage:
/// - Reader: Read-only operations
/// - Writer: Write operations
/// - ReaderWriter: Combined read-write
/// - Store: Store that can create transactions
/// - Txn: Transaction interface with ACID guarantees
/// - TxnStore: Store that supports transactions
///
/// All traits use async_trait to support asynchronous operations.
///
/// # WASM Compatibility
///
/// For WASM targets, all `Send + Sync` bounds are removed since WASM is
/// single-threaded. The `#[cfg_attr]` pattern is used to conditionally
/// apply `async_trait(?Send)` for WASM builds.
use bytes::Bytes;

/// Sealed trait module preventing external implementations of core storage traits.
pub mod private {
    pub trait Sealed {}

    impl Sealed for Box<dyn super::Txn> {}
    impl Sealed for Box<dyn super::super::iterator::Iterator> {}
}

use super::callbacks::{BeforeCommit, OnAbort, OnCommit};
use super::errors::Result;
use super::iterator::Iterator;
use super::retry::{RetryAttempt, RetryAttemptError, RetryAttemptValue, RetryOptions};
use super::types::IterOptions;

pub use defra_core::thread_bounds::{MaybeSend, MaybeSendSync, MaybeSync};

/// Reader trait for read-only key-value operations.
///
/// This trait provides the core read operations: get, has, and iterator.
/// All operations are asynchronous to support various backend implementations.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Reader: MaybeSendSync + private::Sealed {
    /// Retrieve the value associated with a key.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up
    ///
    /// # Returns
    ///
    /// * `Ok(Some(value))` if the key exists
    /// * `Ok(None)` if the key does not exist
    /// * `Err(Error)` if an error occurred
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>>;

    /// Read a value while validating only the named logical parts at commit.
    /// Part IDs belong to the value's merge operator. An empty list requests
    /// an unvalidated read; it must not drive an authoritative decision.
    /// Backends without projection support conservatively validate the whole value.
    async fn get_parts(&self, key: &[u8], _parts: &[u32]) -> Result<Option<Bytes>> {
        self.get(key).await
    }

    /// Check if a key exists in the store.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to check
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if the key exists
    /// * `Ok(false)` if the key does not exist
    /// * `Err(Error)` if an error occurred
    ///
    /// # Note
    ///
    /// Performance may vary by backend. Some backends may need to fetch the value
    /// to check existence.
    async fn has(&self, key: &[u8]) -> Result<bool>;

    /// Get the size of a value without reading the entire value.
    ///
    /// This is useful for checking the size of large values without incurring
    /// the cost of reading them into memory.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to check
    ///
    /// # Returns
    ///
    /// * `Ok(Some(size))` if the key exists, with size in bytes
    /// * `Ok(None)` if the key does not exist
    /// * `Err(Error)` if an error occurred
    ///
    /// # Note
    ///
    /// Some backends may need to read the entire value to determine its size.
    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>>;

    /// Create an iterator over key-value pairs.
    ///
    /// The iterator can be configured with various options for filtering
    /// and ordering using `IterOptions`.
    ///
    /// # Arguments
    ///
    /// * `opts` - Configuration options for the iterator
    ///
    /// # Returns
    ///
    /// * `Ok(Iterator)` - A new iterator instance
    /// * `Err(Error)` if the iterator could not be created
    ///
    /// # Note
    ///
    /// The caller is responsible for closing the iterator when done.
    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>>;
}

/// Writer trait for write operations.
///
/// Every write has two forms. The `_shared` one takes `&self`, as the engine
/// takes its writes: it is how the handles that share one transaction (its
/// namespace views) write without a lock, and it is what an implementation
/// provides. The plain one takes `&mut self`, for a handle its caller holds
/// alone, and does the same write.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Writer: MaybeSendSync + private::Sealed {
    /// [`merge`](Self::merge) through a shared reference.
    async fn merge_shared(&self, _key: &[u8], _operand: &[u8]) -> Result<()> {
        Err(super::errors::Error::Other(
            "value merges are unsupported by this store".into(),
        ))
    }

    /// [`append`](Self::append) through a shared reference.
    async fn append_shared(
        &self,
        _log: &super::types::AppendLog,
        _entry: &[u8],
        _once_key: Option<&[u8]>,
    ) -> Result<()> {
        Err(super::errors::Error::Other(
            "commit-ordered append is unsupported by this store".into(),
        ))
    }

    /// [`truncate_log`](Self::truncate_log) through a shared reference.
    async fn truncate_log_shared(
        &self,
        _log: &super::types::AppendLog,
        _positions: std::ops::RangeInclusive<u64>,
    ) -> Result<()> {
        Err(super::errors::Error::Other(
            "commit-ordered log truncate is unsupported by this store".into(),
        ))
    }

    /// The entries this transaction appended to `log` and has not committed,
    /// oldest first. They have no position yet, so no read of the log shows
    /// them. A read-only transaction has none.
    async fn appended(&self, _log: &super::types::AppendLog) -> Result<Vec<Vec<u8>>> {
        Err(super::errors::Error::Other(
            "commit-ordered append is unsupported by this store".into(),
        ))
    }

    /// [`set`](Self::set) through a shared reference.
    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()>;

    /// [`delete`](Self::delete) through a shared reference.
    async fn delete_shared(&self, key: &[u8]) -> Result<()>;

    /// Apply an operator-defined operand without reading the value first.
    async fn merge(&mut self, key: &[u8], operand: &[u8]) -> Result<()> {
        self.merge_shared(key, operand).await
    }

    /// Queue an entry whose position becomes visible only after commit.
    async fn append(
        &mut self,
        log: &super::types::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        self.append_shared(log, entry, once_key).await
    }

    /// Drops the log's entries at `positions` when the transaction commits;
    /// the head stays, so positions keep counting. Never a conflict.
    async fn truncate_log(
        &mut self,
        log: &super::types::AppendLog,
        positions: std::ops::RangeInclusive<u64>,
    ) -> Result<()> {
        self.truncate_log_shared(log, positions).await
    }

    /// Store a key-value pair.
    ///
    /// If the key already exists, its value is overwritten.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to store (must not be empty)
    /// * `value` - The value to store
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(Error::EmptyKey)` if the key is empty
    /// * `Err(Error::ReadOnlyTxn)` if called on a read-only transaction
    /// * `Err(Error)` for other errors
    async fn set(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        self.set_shared(key, value).await
    }

    /// Delete a key from the store.
    ///
    /// If the key doesn't exist, this is a no-op (not an error).
    ///
    /// # Arguments
    ///
    /// * `key` - The key to delete
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(Error::EmptyKey)` if the key is empty
    /// * `Err(Error::ReadOnlyTxn)` if called on a read-only transaction
    /// * `Err(Error)` for other errors
    async fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.delete_shared(key).await
    }
}

/// ReaderWriter trait combining read and write operations.
///
/// This is a marker trait that combines Reader and Writer.
/// Most stores and transactions implement this trait.
pub trait ReaderWriter: Reader + Writer {}

/// Blanket implementation: any type that implements both Reader and Writer
/// automatically implements ReaderWriter.
impl<T> ReaderWriter for T where T: Reader + Writer {}

/// Store trait for key-value stores that support transactions.
///
/// This trait defines the basic store interface with transaction support.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Store: MaybeSendSync + private::Sealed {
    /// Reserve IDs outside a transaction. Reservations survive aborted mutations.
    /// Store wrappers must forward this capability; database schema and document
    /// creation depend on it, so omission must fail at compile time.
    async fn allocate(&self, key: &[u8], count: u64) -> Result<std::ops::Range<u64>>;

    /// The CRDT types of the node the store belongs to, which its merge
    /// operator folds document parts by. Store wrappers must forward it, so a
    /// node's reads and its store's merges never disagree on a type.
    fn crdts(&self) -> &crdt::Crdts;

    /// Transaction diagnostics for stores that keep them.
    fn transaction_stats_handle(&self) -> Option<crate::backends::TransactionStatsHandle> {
        None
    }

    /// Create a new transaction.
    ///
    /// # Arguments
    ///
    /// * `readonly` - If true, creates a read-only transaction that cannot
    ///   perform writes. Read-only transactions may have better performance.
    ///
    /// # Returns
    ///
    /// * `Ok(Txn)` - A new transaction instance
    /// * `Err(Error::DBClosed)` if the store is closed
    /// * `Err(Error)` for other errors
    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>>;

    /// Run an attempt in a transaction owned and committed by the store.
    /// Only commit conflicts are retried; an error returned by the operation
    /// is returned immediately. The restricted view cannot commit or escape.
    async fn transact_retry(
        &self,
        options: RetryOptions,
        attempt: RetryAttempt,
    ) -> std::result::Result<RetryAttemptValue, RetryAttemptError>;

    /// Close the store and release resources.
    ///
    /// After closing, no further operations can be performed on this store.
    /// Attempting to use a closed store will return `Error::DBClosed`.
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(Error)` if an error occurred during close
    async fn close(&self) -> Result<()>;
}

/// Dropable trait for stores that support bulk deletion.
///
/// This is an optional interface implemented by some stores. It provides a
/// convenient and efficient way of deleting all data in a store.
///
/// # Note
///
/// Concurrent writes will be blocked during `drop_all()`. Depending on the
/// underlying store implementation, concurrent reads may or may not be blocked.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Dropable: Store + private::Sealed {
    /// Delete all data stored in the store.
    ///
    /// This is a destructive operation that removes all key-value pairs.
    /// Use with caution in production environments.
    ///
    /// # Returns
    ///
    /// * `Ok(())` on success
    /// * `Err(Error::DBClosed)` if the store is closed
    /// * `Err(Error)` for other errors
    async fn drop_all(&self) -> Result<()>;
}

/// Transaction trait with ACID guarantees and callback support.
///
/// Transactions provide atomicity, consistency, isolation, and durability (ACID).
/// All operations within a transaction are isolated from other transactions until
/// commit is called.
///
/// # Callbacks
///
/// [`on_commit`](Txn::on_commit) and [`on_abort`](Txn::on_abort) register
/// what runs once the transaction ends: after a commit, or after any other
/// end. The [`callbacks`](super::callbacks) module says in what order.
///
/// # Callback Panic Handling
///
/// Transaction callbacks run directly. In dev/test profiles, a callback panic
/// unwinds through the transaction API. In release builds this workspace uses
/// `panic = "abort"`, so a panicking callback aborts the process.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Txn: ReaderWriter + private::Sealed {
    /// Commit the transaction, making all changes permanent.
    ///
    /// On success the commit callbacks run; on failure the abort callbacks
    /// are awaited.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the transaction committed successfully
    /// * `Err(Error::TxnConflict)` if a conflict occurred (retriable)
    /// * `Err(Error::DiscardedTxn)` if the transaction was already discarded
    /// * `Err(Error)` for other errors
    ///
    /// # Callback Panic Handling
    ///
    /// Callback panics are not recovered here. In dev/test profiles the panic
    /// unwinds through `commit()`. In release builds this workspace uses
    /// `panic = "abort"`, so a panicking callback aborts the process.
    ///
    /// # Note
    ///
    /// After calling commit (success or failure), the transaction cannot be reused.
    async fn commit(self: Box<Self>) -> Result<()>;

    /// Save the current buffered writes and the registered callbacks.
    /// Backends without statement savepoint support fail explicitly.
    fn set_savepoint(&mut self) -> Result<()> {
        Err(crate::Error::Other(
            "transaction savepoints are unsupported by this backend".into(),
        ))
    }

    /// Roll back writes and callbacks staged since the latest savepoint.
    fn rollback_to_savepoint(&mut self) -> Result<()> {
        Err(crate::Error::Other(
            "transaction savepoints are unsupported by this backend".into(),
        ))
    }

    /// Release the latest savepoint, retaining its writes and callbacks.
    fn release_savepoint(&mut self) -> Result<()> {
        Err(crate::Error::Other(
            "transaction savepoints are unsupported by this backend".into(),
        ))
    }

    /// Discard the transaction, rolling back all changes.
    ///
    /// The abort callbacks run, the async ones spawned as background tasks
    /// that may not finish if the process exits first: `discard()` is
    /// synchronous, as in Go DefraDB. After calling discard, the transaction
    /// cannot be used for any further operations.
    ///
    /// # Callback Panic Handling
    ///
    /// Callback panics are not recovered here. In dev/test profiles the panic
    /// unwinds through `discard()`. In release builds this workspace uses
    /// `panic = "abort"`, so a panicking callback aborts the process.
    ///
    /// # Note
    ///
    /// This method consumes self, ensuring the transaction cannot be used after discard.
    fn discard(self: Box<Self>);

    /// Register `callback` to run once if the transaction commits.
    ///
    /// Plain callbacks run once the commit is durable. An ordered one goes
    /// out with every commit's events in commit-receipt order, so it needs a
    /// commit receipt: a read-only transaction refuses it with
    /// [`Error::ReadOnlyTxn`](super::Error::ReadOnlyTxn).
    ///
    /// ```no_run
    /// use storage::corekv::{OnAbort, OnCommit, Store};
    ///
    /// # async fn example(store: storage::RegolithStore) -> storage::Result<()> {
    /// let mut txn = store.new_txn(false).await?;
    /// txn.set(b"doc", b"v1").await?;
    /// // Once the commit is durable.
    /// txn.on_commit(OnCommit::Sync(Box::new(|| println!("committed"))))?;
    /// // With every commit's events, in commit-receipt order.
    /// txn.on_commit(OnCommit::Ordered(Box::new(|| println!("doc changed"))))?;
    /// // Before this commit's events go out; a failure is reported and the
    /// // commit stands.
    /// txn.on_commit(OnCommit::OrderedAsync(Box::new(|| Box::pin(async { Ok(()) }))))?;
    /// // Only if the transaction ends without committing.
    /// txn.on_abort(OnAbort::Sync(Box::new(|| println!("rolled back"))));
    /// txn.commit().await
    /// # }
    /// ```
    fn on_commit(&mut self, callback: OnCommit) -> Result<()>;

    /// Register `callback` to run once if the transaction ends without
    /// committing: its commit fails, or it is discarded or dropped.
    fn on_abort(&mut self, callback: OnAbort);

    /// Register `callback` to run once just before the commit is attempted.
    /// An error from it fails the commit, and the abort callbacks run.
    fn before_commit(&mut self, callback: BeforeCommit);

    /// Downcast to concrete type (for internal use in tests)
    fn as_any(&self) -> &dyn std::any::Any;

    /// Downcast to concrete type (for internal use in tests)
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Check if this is a read-only transaction.
    fn is_readonly(&self) -> bool;

    /// Get the total number of callbacks registered on this transaction, of
    /// every kind.
    ///
    /// Useful for monitoring callback accumulation in long-running transactions.
    fn callback_count(&self) -> usize;
}

/// TxnStore trait for stores that support transactions.
///
/// This is a marker trait combining Store with the ability to create transactions.
/// Most production stores implement this trait.
pub trait TxnStore: Store {}

/// Blanket implementation: any Store automatically implements TxnStore.
impl<T> TxnStore for T where T: Store {}

/// Blanket implementation of Reader for Box<dyn Txn>.
///
/// This allows boxed transactions to be used where Reader is required.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Reader for Box<dyn Txn> {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        (**self).get(key).await
    }

    async fn get_parts(&self, key: &[u8], parts: &[u32]) -> Result<Option<Bytes>> {
        (**self).get_parts(key, parts).await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        (**self).has(key).await
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        (**self).get_size(key).await
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        (**self).iterator(opts).await
    }
}

/// Blanket implementation of Writer for Box<dyn Txn>.
///
/// This allows boxed transactions to be used where Writer is required.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Writer for Box<dyn Txn> {
    async fn merge_shared(&self, key: &[u8], operand: &[u8]) -> Result<()> {
        (**self).merge_shared(key, operand).await
    }

    async fn append_shared(
        &self,
        log: &super::types::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        (**self).append_shared(log, entry, once_key).await
    }

    async fn truncate_log_shared(
        &self,
        log: &super::types::AppendLog,
        positions: std::ops::RangeInclusive<u64>,
    ) -> Result<()> {
        (**self).truncate_log_shared(log, positions).await
    }

    async fn appended(&self, log: &super::types::AppendLog) -> Result<Vec<Vec<u8>>> {
        (**self).appended(log).await
    }

    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()> {
        (**self).set_shared(key, value).await
    }

    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        (**self).delete_shared(key).await
    }

    // The plain forms forward as themselves: through the defaults each would
    // box a future here, another for the shared form, and a third inside.
    async fn merge(&mut self, key: &[u8], operand: &[u8]) -> Result<()> {
        (**self).merge(key, operand).await
    }

    async fn append(
        &mut self,
        log: &super::types::AppendLog,
        entry: &[u8],
        once_key: Option<&[u8]>,
    ) -> Result<()> {
        (**self).append(log, entry, once_key).await
    }

    async fn truncate_log(
        &mut self,
        log: &super::types::AppendLog,
        positions: std::ops::RangeInclusive<u64>,
    ) -> Result<()> {
        (**self).truncate_log(log, positions).await
    }

    async fn set(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        (**self).set(key, value).await
    }

    async fn delete(&mut self, key: &[u8]) -> Result<()> {
        (**self).delete(key).await
    }
}
