//! The loop that drives a sent request to its receipt, asking again at each
//! newly verified height. The node serves a receipt from the moment it
//! publishes the header of the block that holds it, so a wait on headers
//! asks as soon as the receipt can be there. Modeled by
//! proofs/tla/ReceiptWait.tla.

use std::future::Future;
use std::time::Duration;

use acp_light_client::AcpLightClient;
use defra_core::thread_bounds::MaybeBoxFuture;

/// The shortest fallback a wait is given: an ask the node cannot answer
/// costs it a lookup of its finalized history.
pub(crate) const MIN_FALLBACK: Duration = Duration::from_millis(5);

/// The newest verified height, and a wait for a higher one.
pub(crate) trait Heads: Send + Sync {
    fn height(&self) -> u64;

    /// Ends once a height above `seen` is verified, or after `bound`.
    fn above(&self, seen: u64, bound: Duration) -> MaybeBoxFuture<'_, ()>;
}

impl Heads for AcpLightClient {
    fn height(&self) -> u64 {
        self.header_chain().latest_height()
    }

    fn above(&self, seen: u64, bound: Duration) -> MaybeBoxFuture<'_, ()> {
        Box::pin(async move {
            // A wait that ends unmet asks all the same: the bound is the fallback.
            let _ = edge::compat(self.wait_for_height(seen.saturating_add(1), bound)).await;
        })
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
/// at most `fallback` when no header comes.
pub(crate) async fn until_receipt<T, E, Ask, Asked, Send, Sent>(
    heads: &dyn Heads,
    fallback: Duration,
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
        heads.above(seen, fallback).await;
    }
}

#[cfg(test)]
#[path = "receipt_wait_tests.rs"]
mod tests;
