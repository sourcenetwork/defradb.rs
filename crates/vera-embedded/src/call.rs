//! One call into the node's method table, under the caller's byte limit.

use serde_json::Value;
use vera_jsonrpc::LocalRpc;

use crate::error::{EmbeddedError, Failure};

/// Calls `method` and returns its `result`, refusing one whose JSON text is
/// longer than `max_bytes`.
///
/// The network transports apply the limit to the whole response body, which
/// is this text inside its JSON-RPC envelope. A result they accept is
/// therefore accepted here.
pub(crate) async fn call(
    rpc: &LocalRpc,
    method: &str,
    params: Value,
    max_bytes: usize,
) -> Result<Value, Failure> {
    let params = positional(method, params).map_err(Failure::Channel)?;
    let result = rpc
        .request_raw(method, params)
        .await
        .map_err(|error| Failure::from_node(method, error))?;
    decode(&result, max_bytes)
}

/// The parameters as the node takes them: by position.
fn positional(method: &str, params: Value) -> Result<Vec<Value>, EmbeddedError> {
    let found = match params {
        Value::Array(params) => return Ok(params),
        Value::Null => return Ok(Vec::new()),
        Value::Object(_) => "an object",
        Value::String(_) => "a string",
        Value::Number(_) => "a number",
        Value::Bool(_) => "a boolean",
    };
    Err(EmbeddedError::Params {
        method: method.to_owned(),
        found,
    })
}

/// Whether a result of `length` bytes is within the caller's limit.
pub(crate) fn within(length: usize, max_bytes: usize) -> bool {
    length <= max_bytes
}

fn decode(result: &str, max_bytes: usize) -> Result<Value, Failure> {
    if !within(result.len(), max_bytes) {
        return Err(Failure::TooLarge);
    }
    serde_json::from_str(result).map_err(Failure::Malformed)
}

#[cfg(test)]
mod tests;
