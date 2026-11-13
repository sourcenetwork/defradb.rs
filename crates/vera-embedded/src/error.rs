//! How an in-process call can fail, and what each consumer is told.

use std::time::Duration;

use vera_jsonrpc::LocalRpcError;

/// A failure of the in-process channel, as distinct from an answer the node
/// gave.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EmbeddedError {
    /// The node this transport reaches has stopped. Asking again cannot
    /// succeed: a restarted node has a method table of its own.
    #[error("{method}: the embedded Vera node has stopped")]
    Stopped { method: String },

    /// The node gave no answer within the deadline. It may still finish the
    /// method: only the wait ended, so asking again is safe and may succeed.
    #[error("{method}: no answer from the embedded Vera node within {deadline:?}")]
    Elapsed { method: String, deadline: Duration },

    /// The network runtime that times the wait is stopping.
    #[error("{method}: {reason}")]
    Runtime {
        method: String,
        #[source]
        reason: edge::EdgeError,
    },

    /// The call's parameters cannot be sent by position.
    #[error("{method}: parameters must be a JSON array, got {found}")]
    Params { method: String, found: &'static str },

    /// The parameters or a notification could not be encoded or decoded.
    #[error("{method}: invalid JSON-RPC payload: {reason}")]
    Payload { method: String, reason: String },

    /// The node did not accept the subscription.
    #[error("{method}: subscription failed: {reason}")]
    Subscription { method: String, reason: String },
}

/// Why a call produced no result.
#[derive(Debug)]
pub(crate) enum Failure {
    /// The node answered with a JSON-RPC error, the one a network client gets.
    Node { code: i32, message: String },
    /// The result is larger than the caller allows.
    TooLarge,
    /// The result is not JSON.
    Malformed(serde_json::Error),
    /// The node gave no answer within the deadline.
    Elapsed { method: String, deadline: Duration },
    /// The call did not reach the node or its answer did not come back.
    Channel(EmbeddedError),
}

impl Failure {
    /// The error a caller reads when the wait ended, for either consumer.
    pub(crate) fn elapsed(method: String, deadline: Duration) -> EmbeddedError {
        EmbeddedError::Elapsed { method, deadline }
    }

    pub(crate) fn from_node(method: &str, error: LocalRpcError) -> Self {
        let method = method.to_owned();
        match error {
            LocalRpcError::Call { code, message, .. } => Self::Node { code, message },
            LocalRpcError::Payload(reason) => {
                Self::Channel(EmbeddedError::Payload { method, reason })
            }
            LocalRpcError::Subscription(reason) => {
                Self::Channel(EmbeddedError::Subscription { method, reason })
            }
            LocalRpcError::Stopped => Self::Channel(EmbeddedError::Stopped { method }),
        }
    }
}
