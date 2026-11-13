//! A hook an index backfill passes at each batch boundary, so a test can hold
//! a build half done. Go's `db.IndexBuildGate`.

use std::future::Future;
use std::pin::Pin;

/// What [`IndexBuildGate::enter`] returns: done when the batch may run.
pub type GateWait<'gate> = Pin<Box<dyn Future<Output = ()> + Send + 'gate>>;

/// Holds an index backfill before each of its batches until the gate lets
/// it through. The wait is an `.await`, never a blocked thread; a build the
/// database stops while it waits is dropped with the wait.
pub trait IndexBuildGate: Send + Sync {
    /// Called before each batch of the build of index `index_id` on the
    /// collection `collection_id`.
    fn enter<'gate>(&'gate self, collection_id: &'gate str, index_id: u32) -> GateWait<'gate>;
}
