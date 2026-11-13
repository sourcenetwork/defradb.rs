//! The light client's transport: proof reads and finalized headers.

use futures::future::BoxFuture;
use futures::{FutureExt as _, StreamExt as _};
use serde_json::Value;
use vera::vera_rs::{AcpTransport, HeaderStream, TransportError};

use crate::call::call;
use crate::deadline::{self, bounded};
use crate::error::{EmbeddedError, Failure};
use crate::EmbeddedTransport;

const SUBSCRIBE_HEADERS: &str = "vera_subscribeHeaders";

/// Headers the node may queue for a light client that is busy verifying one.
/// The network server queues as many messages per connection. A client that
/// falls further behind loses the subscription and subscribes again.
const HEADER_BUFFER: usize = 8;

impl AcpTransport for EmbeddedTransport {
    fn call(
        &self,
        method: &'static str,
        params: Value,
        max_bytes: usize,
    ) -> BoxFuture<'_, Result<Value, TransportError>> {
        async move {
            call(self, method, params, max_bytes, deadline::PROOF_READ)
                .await
                .map_err(|failure| transport_error(method, failure))
        }
        .boxed()
    }

    fn subscribe_headers(&self) -> BoxFuture<'_, Result<HeaderStream, TransportError>> {
        async move {
            let accepted = self
                .rpc
                .subscribe(SUBSCRIBE_HEADERS, Vec::new(), HEADER_BUFFER);
            let subscription = bounded(
                &self.edge,
                SUBSCRIBE_HEADERS,
                deadline::SUBSCRIPTION,
                accepted,
            )
            .await
            .map_err(header_error)?
            .map_err(|error| header_error(Failure::from_node(SUBSCRIBE_HEADERS, error)))?;
            let headers: HeaderStream =
                futures::stream::unfold(subscription, |mut subscription| async move {
                    let header = subscription.next().await?.map_err(|error| {
                        header_error(Failure::from_node(SUBSCRIBE_HEADERS, error))
                    });
                    Some((header, subscription))
                })
                .boxed();
            Ok(headers)
        }
        .boxed()
    }
}

fn header_error(failure: Failure) -> TransportError {
    transport_error(SUBSCRIBE_HEADERS, failure)
}

/// What the light client is told, in the terms its HTTP transport uses for
/// the same outcome.
pub(crate) fn transport_error(method: &'static str, failure: Failure) -> TransportError {
    match failure {
        Failure::Node { code, message } => TransportError::Rpc {
            method,
            code: i64::from(code),
            message,
        },
        Failure::TooLarge => TransportError::ResponseTooLarge { method },
        Failure::Malformed(error) => TransportError::other(error),
        Failure::Elapsed { method, deadline } => {
            TransportError::other(EmbeddedError::Elapsed { method, deadline })
        }
        Failure::Channel(error) => TransportError::other(error),
    }
}

#[cfg(test)]
mod tests;
