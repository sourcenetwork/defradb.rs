//! The loop that drives a sent request to its receipt, asking again at each
//! newly verified height. The node serves a receipt from the moment it
//! publishes the header of the block that holds it, so a wait on headers
//! asks as soon as the receipt can be there. Modeled by
//! proofs/tla/ReceiptWait.tla.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use acp_light_client::AcpLightClient;
use defra_core::thread_bounds::MaybeBoxFuture;

/// The shortest fallback a wait is given: an ask the node cannot answer
/// costs it a lookup of its finalized history.
const MIN_FALLBACK: Duration = Duration::from_millis(5);

/// The newest verified height, and a wait for a higher one.
pub(crate) trait Heads: Send + Sync {
    fn height(&self) -> u64;

    /// Ends once a height above `seen` is verified, answering `true`, or
    /// after `bound`, answering `false`.
    fn above(&self, seen: u64, bound: Duration) -> MaybeBoxFuture<'_, bool>;
}

impl Heads for AcpLightClient {
    fn height(&self) -> u64 {
        self.header_chain().latest_height()
    }

    fn above(&self, seen: u64, bound: Duration) -> MaybeBoxFuture<'_, bool> {
        Box::pin(async move {
            edge::compat(self.wait_for_height(seen.saturating_add(1), bound))
                .await
                .is_ok()
        })
    }
}

/// How long a wait lasts with no new header, and how many waits ran that long.
#[derive(Debug)]
pub(crate) struct Fallback {
    bound: Duration,
    expired: AtomicU64,
}

impl Fallback {
    /// A fallback of `bound`, at least [`MIN_FALLBACK`].
    pub(crate) fn new(bound: Duration) -> Self {
        Self {
            bound: bound.max(MIN_FALLBACK),
            expired: AtomicU64::new(0),
        }
    }

    /// How many waits ended at the bound rather than at a new header.
    pub(crate) fn expired(&self) -> u64 {
        self.expired.load(Ordering::Relaxed)
    }
}

/// What a loop does first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Start {
    /// Ask for the receipt: the request may have been sent before.
    Ask,
    /// Send: the request was never sent, so the node has no receipt for it.
    Send,
}

/// Asks through `ask` until it returns a receipt, sending through `send`
/// until a send is known to have reached the node. `ask` answers `None` for
/// no receipt yet and `send` `false` for a send that may not have arrived;
/// either ends the loop with an error that cannot pass. Between turns the
/// loop waits for a height above the one it read before its ask and send, so
/// a header verified while either was on its way ends the wait at once, and
/// at most `fallback`'s bound when no header comes, which `fallback` counts.
pub(crate) async fn until_receipt<T, E, Ask, Asked, Send, Sent>(
    heads: &dyn Heads,
    fallback: &Fallback,
    start: Start,
    mut ask: Ask,
    mut send: Send,
) -> Result<T, E>
where
    Ask: FnMut() -> Asked,
    Asked: Future<Output = Result<Option<T>, E>>,
    Send: FnMut() -> Sent,
    Sent: Future<Output = Result<bool, E>>,
{
    let mut asks = start == Start::Ask;
    let mut sent = false;
    loop {
        let seen = heads.height();
        if asks {
            if let Some(receipt) = ask().await? {
                return Ok(receipt);
            }
        }
        asks = true;
        if !sent {
            sent = send().await?;
        }
        if !heads.above(seen, fallback.bound).await {
            fallback.expired.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
#[path = "receipt_wait_tests.rs"]
mod tests;
