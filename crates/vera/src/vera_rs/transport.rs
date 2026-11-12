use defra_core::thread_bounds::{MaybeBoxFuture, MaybeSendSync};

use super::client::ClientError;

/// A JSON-RPC channel to a vera.rs node.
///
/// The transport only carries the call. Callers still check what comes back:
/// a submission ID against the signed bytes, a receipt against the trusted key.
pub trait RpcTransport: MaybeSendSync + 'static {
    /// Calls `method` with positional `params` and returns its `result`,
    /// rejecting a response larger than `max_bytes`.
    fn call<'a>(
        &'a self,
        method: &'a str,
        params: serde_json::Value,
        max_bytes: usize,
    ) -> MaybeBoxFuture<'a, Result<serde_json::Value, ClientError>>;
}
