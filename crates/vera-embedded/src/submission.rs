//! The submission transport: native transactions and their receipts.

use defra_core::thread_bounds::MaybeBoxFuture;
use serde_json::Value;
use vera::vera_rs::{ClientError, RpcTransport};

use crate::call::call;
use crate::error::Failure;
use crate::EmbeddedTransport;

impl RpcTransport for EmbeddedTransport {
    fn call<'a>(
        &'a self,
        method: &'a str,
        params: Value,
        max_bytes: usize,
    ) -> MaybeBoxFuture<'a, Result<Value, ClientError>> {
        Box::pin(async move {
            call(&self.rpc, method, params, max_bytes)
                .await
                .map_err(client_error)
        })
    }
}

/// What the submission client is told, in the terms the HTTP transport uses
/// for the same outcome. A node error keeps its code, so a busy node
/// (`-32002`) is asked again as it is over HTTP. A stopped node is not: the
/// request stays in the worker's journal for the next node.
pub(crate) fn client_error(failure: Failure) -> ClientError {
    match failure {
        Failure::Node { code, message } => ClientError::Rpc {
            code: i64::from(code),
            message,
        },
        Failure::TooLarge => ClientError::InvalidResponse("response exceeds byte limit"),
        Failure::Malformed(error) => ClientError::Json(error),
        Failure::Channel(error) => ClientError::Transport {
            retryable: false,
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests;
