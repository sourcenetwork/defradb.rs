//! Shared request/response types for HTTP client

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// GraphQL request body
#[derive(Debug, Serialize)]
pub struct GraphQLRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<JsonValue>,
    #[serde(rename = "operationName", skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txn_id: Option<String>,
}

/// GraphQL response from server
#[derive(Debug, Deserialize)]
pub struct GraphQLResponse {
    pub data: Option<JsonValue>,
    #[serde(default)]
    pub errors: Vec<GraphQLError>,
    /// Extra information about a request that worked, such as warnings.
    #[serde(default)]
    pub extensions: Option<JsonValue>,
}

/// GraphQL error: its message, and the server's extensions when it sent
/// some. Go's client prints the message only; the printed `extensions` are
/// ours.
#[derive(Debug, Deserialize, Serialize)]
pub struct GraphQLError {
    pub message: String,
    /// The server's machine-readable error code, which Go's error mapping reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<GraphQLErrorExtensions>,
}

/// GraphQL error extensions: the server's own, so every field it sends is kept.
pub type GraphQLErrorExtensions = query::executor::QueryResponseErrorExtensions;

/// Transaction begin response with the opaque ID later routes carry.
#[derive(Debug, Deserialize)]
pub struct TxBeginResponse {
    pub id: u64,
}

/// Server error response
#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    /// The server's machine-readable error code, such as `STORAGE_FULL`.
    #[serde(default)]
    pub code: Option<String>,
    /// The commit outcome of a failed write (`aborted`, `indeterminate`).
    #[serde(default)]
    pub outcome: Option<String>,
}

impl ErrorResponse {
    /// The client error for this body: [`Error::Server`] when it carries
    /// only text, [`Error::Reply`] when it carries a code or an outcome.
    pub fn into_error(self) -> crate::error::Error {
        reply_error(self.error, self.code, self.outcome.as_deref())
    }
}

/// [`crate::error::Error::Server`] for bare text, `Reply` with the code and the
/// outcome a wire value names otherwise.
pub fn reply_error(
    message: String,
    code: Option<String>,
    outcome: Option<&str>,
) -> crate::error::Error {
    let outcome = outcome.and_then(defra_core::CommitOutcome::from_wire);
    match (code, outcome) {
        (None, None) => crate::error::Error::Server(message),
        (code, outcome) => crate::error::Error::Reply {
            message,
            code,
            outcome,
        },
    }
}

impl GraphQLResponse {
    /// Check if the response has errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Format errors as a string
    pub fn error_message(&self) -> String {
        self.errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    }
}
