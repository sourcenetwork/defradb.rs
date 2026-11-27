/// Traits for the datastore layer matching Go's internal/datastore/txn.go.
use crate::multistore::{NamespaceView, RootView};
use storage::corekv::{MaybeSendSync, OnAbort, OnCommit};

/// Transaction trait for DefraDB operations.
///
/// This matches Go's Txn interface in internal/datastore/txn.go.
/// It provides access to all namespaced stores and transaction lifecycle methods.
pub trait Txn: MaybeSendSync {
    /// Get the blockstore (namespace 'b').
    fn blockstore(&self) -> NamespaceView;

    /// Get the datastore (namespace 'd').
    fn datastore(&self) -> NamespaceView;

    /// Get the encstore (namespace 'e').
    fn encstore(&self) -> NamespaceView;

    /// Get the headstore (namespace 'h').
    fn headstore(&self) -> NamespaceView;

    /// Get the peerstore (namespace 'p').
    fn peerstore(&self) -> NamespaceView;

    /// Get the systemstore (namespace 's').
    fn systemstore(&self) -> NamespaceView;

    /// Get the rootstore (no namespace prefix).
    fn rootstore(&self) -> RootView;

    /// Get the unique transaction ID.
    fn id(&self) -> u64;

    /// Check if this is a read-only transaction.
    fn is_readonly(&self) -> bool;

    /// Register `callback` to run once if the transaction commits.
    fn on_commit(&mut self, callback: OnCommit) -> storage::corekv::Result<()>;

    /// Register `callback` to run once if the transaction ends without
    /// committing.
    fn on_abort(&mut self, callback: OnAbort);

    /// Register `callback` to run once just before the commit is attempted.
    /// An error from it fails the commit, and the abort callbacks run.
    fn before_commit(&mut self, callback: storage::corekv::BeforeCommit);
}
