/// CoreKV abstraction layer for key-value storage.
///
/// This module provides a complete abstraction layer for key-value storage operations,
/// inspired by the Go corekv package. It defines a set of traits that allow for multiple
/// backend implementations (Redb, Memory, etc.) while providing a consistent API.
///
/// # Architecture
///
/// The CoreKV layer follows a hierarchical trait structure:
///
/// ```text
/// Store
///   ├── Reader (get, has, iterator)
///   ├── Writer (set, delete)
///   └── ReaderWriter (combines Reader + Writer)
///
/// Txn (extends ReaderWriter)
///   ├── commit/discard
///   └── callbacks (on_commit, on_abort)
///
/// TxnStore (extends Store)
///   └── new_txn(readonly) -> Txn
/// ```
///
/// # Key Features
///
/// - **Async-first**: All I/O operations are asynchronous using async/await
/// - **MVCC Transactions**: Support for serializable snapshot isolation
/// - **Callbacks**: One registry per transaction, run once at its end (`callbacks`)
/// - **Iteration**: Flexible iteration with prefix, range, and reverse support
/// - **Backend Agnostic**: Works with Redb, in-memory, or custom backends
///
/// # Example
///
/// ```ignore
/// use storage::corekv::{Store, Reader, Writer, IterOptions};
///
/// // Create a store (on disk or in memory)
/// let store = RegolithStore::in_memory().unwrap();
///
/// // Create a transaction
/// let mut txn = store.new_txn(false).await?;
///
/// // Write some data
/// txn.set(b"key1", b"value1").await?;
/// txn.set(b"key2", b"value2").await?;
///
/// // Register a commit callback
/// txn.on_commit(OnCommit::Sync(Box::new(|| println!("Transaction committed!"))))?;
///
/// // Commit the transaction
/// txn.commit().await?;
///
/// // Read back
/// let txn = store.new_txn(true).await?;
/// let value = txn.get(b"key1").await?;
/// assert_eq!(value, Some(Bytes::from_static(b"value1")));
///
/// // Iterate
/// let opts = IterOptions::new().with_prefix(b"key".to_vec());
/// let mut iter = txn.iterator(opts).await?;
/// while let Some(kv) = iter.next().await? {
///     println!("{}: {}", kv.key_str(), kv.value_str());
/// }
/// iter.close().await?;
/// ```
pub mod callbacks;
pub mod errors;
mod io_failure;
pub mod iterator;
pub mod retry;
pub mod traits;
pub mod transact;
pub mod types;

// Re-export commonly used types and traits for convenience
pub use callbacks::{
    make_async_callback, make_callback, AsyncTxnCallback, BeforeCommit, CallOnce, CallbackMark,
    CallbackSink, CommitHook, Committed, Ended, OnAbort, OnCommit, QueuedCallbacks, Registration,
    TxnCallback, TxnCallbacks,
};
pub use errors::{Error, Result, TransactionConflict, UNIQUE_CONSTRAINT_VIOLATION_MESSAGE};
pub use io_failure::IoFailure;
pub use iterator::{Iterator, KvPair};
pub use retry::policy::{engine_retry_options, DEFAULT_MAX_TXN_RETRIES};
pub use retry::{
    intercept_retry_writes, InterceptWrite, RetryAttempt, RetryAttemptError, RetryAttemptFn,
    RetryAttemptFuture, RetryAttemptIO, RetryAttemptValue, RetryOptions, RetryTxn,
    RetryWriteInterceptor,
};
pub use traits::private;
pub use traits::{
    Dropable, MaybeSend, MaybeSendSync, MaybeSync, Reader, ReaderWriter, Store, Txn, TxnStore,
    Writer,
};
pub use transact::transact;
pub use types::{AppendLog, IterOptions, Key};
