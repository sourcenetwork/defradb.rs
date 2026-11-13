/// Datastore layer for DefraDB matching Go's internal/datastore/.
///
/// This crate provides the transaction wrapper layer that sits between
/// the storage layer (corekv) and the database layer (db). It provides:
///
/// - `BasicTxn`: Transaction wrapper with multistore access and callbacks
/// - `Txn` trait: Common interface for DefraDB transactions
/// - `NamespaceView`: View into a specific namespace of a transaction
///
/// # Architecture
///
/// ```text
/// db crate (Database, Collection, DbTxn)
///     ↓
/// datastore crate (BasicTxn, Txn trait, NamespaceView)
///     ↓
/// storage crate (corekv, Multistore, backends)
/// ```
///
/// # Example
///
/// ```ignore
/// use datastore::BasicTxn;
/// use storage::RegolithStore;
///
/// let store = RegolithStore::in_memory().unwrap();
/// let mut txn = BasicTxn::new(&store, 1, false).await?;
///
/// // Access namespaced stores
/// txn.datastore().set(b"key", b"value").await?;
/// txn.systemstore().set(b"config", b"data").await?;
///
/// // Register callbacks
/// txn.on_commit(OnCommit::Sync(Box::new(|| println!("Committed!"))))?;
///
/// // Commit
/// txn.commit().await?;
/// ```
pub mod error;
pub mod multistore;
pub mod traits;
pub mod transact;
pub mod txn;

// Re-export commonly used types
pub use error::{Error, Result};
pub use multistore::{
    DocumentMutationProjector, NamespaceView, NodeAccessWrites, RootView, SharedTxn,
    WeakNamespaceView,
};
pub use traits::Txn;
pub use transact::transact;
pub use txn::BasicTxn;

// Re-export storage types for convenience
pub use storage::corekv::{IterOptions, OnAbort, OnCommit, Store, TxnCallback};
pub use storage::namespace::Namespace;
