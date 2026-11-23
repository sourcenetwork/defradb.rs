//! A hook that holds an explicit transaction commit before it begins.

use std::future::Future;
use std::pin::Pin;

pub type CommitWait<'gate> = Pin<Box<dyn Future<Output = ()> + Send + 'gate>>;

/// Holds an explicit transaction's commit until the gate lets it through.
/// The wait is an `.await`, never a blocked thread.
pub trait CommitGate: Send + Sync {
    fn enter<'gate>(&'gate self) -> CommitWait<'gate>;
}
