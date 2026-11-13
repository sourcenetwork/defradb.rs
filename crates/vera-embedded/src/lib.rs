//! Transports that reach a Vera node hosted in the same process.
//!
//! [`EmbeddedTransport`] carries the vera.rs provider's two channels over the
//! node's in-process method table instead of HTTP and a WebSocket: the
//! submission transport ([`RpcTransport`](vera::vera_rs::RpcTransport)) and
//! the light client's transport
//! ([`AcpTransport`](vera::vera_rs::AcpTransport)). The methods, their
//! limits and their errors are the ones the node's network server gives.
//!
//! Sharing a process is no reason to trust the node: the provider verifies
//! every receipt, proof and header it gets through here against its trusted
//! consensus key, exactly as it does over the network.
//!
//! The node runs its calls on its own runtime. What this crate returns are
//! plain futures that any executor can await.

mod acp;
mod call;
mod error;
mod submission;

pub use error::EmbeddedError;
pub use vera_jsonrpc::LocalRpc;

/// The in-process channel to one embedded Vera node.
///
/// It reaches the node that built `rpc` and no other: once that node stops,
/// every call fails with [`EmbeddedError::Stopped`], and a node started in
/// its place needs a transport of its own.
#[derive(Clone, Debug)]
pub struct EmbeddedTransport {
    rpc: LocalRpc,
}

impl EmbeddedTransport {
    /// A transport over `rpc`, the method table of a running node
    /// (`EmbeddedNode::rpc`).
    pub fn new(rpc: LocalRpc) -> Self {
        Self { rpc }
    }
}
