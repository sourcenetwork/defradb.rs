//! A hook that holds an explicit transaction commit before it is handed to
//! the transaction's owner.

use std::future::Future;
use std::pin::Pin;

pub type CommitWait<'gate> = Pin<Box<dyn Future<Output = ()> + Send + 'gate>>;

/// Holds an explicit transaction's commit until the gate lets it through.
/// The wait is an `.await`, never a blocked thread.
pub trait CommitGate: Send + Sync {
    /// Called each time the commit is handed to the transaction's owner; the
    /// commit waits for the returned future. A commit that first asks for
    /// owner confirmation passes the gate again with its confirmed attempt.
    fn enter<'gate>(&'gate self) -> CommitWait<'gate>;

    /// Called once the commit that passed the gate has its outcome; not when
    /// the owner kept the transaction open to confirm its told owners.
    fn ended(&self);
}
