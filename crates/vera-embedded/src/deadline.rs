//! The wait for the node's answer, bounded.
//!
//! A network transport gives up on a call after a deadline, and the provider
//! and the light client rely on that: neither puts a limit of its own on a
//! read. The in-process calls get the same deadlines, timed on the network
//! runtime's clock through the [`Edge`] the transport is given.

use std::future::Future;
use std::time::Duration;

use edge::{Edge, EdgeError};

use crate::error::{EmbeddedError, Failure};

/// How long a light client read may take: the request timeout of Backbone's
/// HTTP transport (`HttpWsTransport` in acp-light-client, Backbone 502e604).
pub(crate) const PROOF_READ: Duration = Duration::from_secs(10);

/// How long the node may take to accept the header subscription: the
/// acknowledgement deadline of Backbone's WebSocket path
/// (`websocket_headers`, same revision).
pub(crate) const SUBSCRIPTION: Duration = Duration::from_secs(10);

/// `answer`, or [`Failure::Elapsed`] when the node has not given it within
/// `deadline`.
///
/// Only the wait ends. The node runs the method on its own runtime, finishes
/// it there and drops an answer nobody waits for, as it does for a network
/// client that hung up.
pub(crate) async fn bounded<F>(
    edge: &Edge,
    method: &str,
    deadline: Duration,
    answer: F,
) -> Result<F::Output, Failure>
where
    F: Future,
{
    edge.timeout(deadline, answer)
        .await
        .map_err(|error| match error {
            EdgeError::Deadline(deadline) => Failure::Elapsed {
                method: method.to_owned(),
                deadline,
            },
            reason => Failure::Channel(EmbeddedError::Runtime {
                method: method.to_owned(),
                reason,
            }),
        })
}

#[cfg(test)]
mod tests;
