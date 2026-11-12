//! The end of a subscription stream at shutdown.

use std::future::Future;
use std::pin::{pin, Pin};

use futures::future::{select, Either};

/// `next`'s output, or `None` once `closing` completes, which wins a tie.
///
/// A subscription's response has no end of its own: the server's graceful
/// shutdown would wait for it until the drain's deadline. A stream that reads
/// its next item through this, with its edge's `closing()`, ends instead as
/// soon as the edge stops admitting requests (proofs/tla/ServerDrain.tla
/// EndStream). `closing` must not be polled again once it completed.
pub(crate) async fn next_unless_closing<C, F, T>(closing: Pin<&mut C>, next: F) -> Option<T>
where
    C: Future<Output = ()>,
    F: Future<Output = Option<T>>,
{
    match select(closing, pin!(next)).await {
        Either::Left(((), _)) => None,
        Either::Right((next, _)) => next,
    }
}
