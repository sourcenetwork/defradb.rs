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

// The layout of the node's start future is deeper than the default limit.
#![cfg_attr(feature = "node", recursion_limit = "256")]

use std::time::Duration;

use edge::Edge;

mod acp;
mod call;
mod deadline;
mod error;
#[cfg(feature = "node")]
mod host;
mod link;
mod submission;

pub use error::EmbeddedError;
#[cfg(feature = "node")]
pub use host::{Deployment, HostConfig, HostError, HostedNode, SeededPolicy, ThreadBudget};
pub use vera_jsonrpc::LocalRpc;

/// The in-process channel to one embedded Vera node.
///
/// It reaches the node that built `rpc` and no other: once that node stops,
/// every call fails with [`EmbeddedError::Stopped`], and a node started in
/// its place needs a transport of its own.
///
/// Every wait for the node has a deadline, the one the network transport in
/// its place has. A submission call waits `request_timeout`. A light client
/// read waits ten seconds and so does establishing the header subscription,
/// as over Backbone's HTTP and WebSocket transport. A wait that ends fails
/// with [`EmbeddedError::Elapsed`]; the node still finishes the method.
#[derive(Clone, Debug)]
pub struct EmbeddedTransport {
    rpc: link::Link,
    request_timeout: Duration,
    edge: Edge,
}

impl EmbeddedTransport {
    /// A transport over `rpc`, the method table of a running node
    /// (`EmbeddedNode::rpc`), whose deadlines run on `edge`'s clock.
    ///
    /// `request_timeout` bounds each submission call, as it does for the
    /// HTTP transport: the provider's `AcpTuning::request_timeout`.
    pub fn new(rpc: LocalRpc, request_timeout: Duration, edge: &Edge) -> Self {
        Self::linked(link::Link::new(rpc), request_timeout, edge)
    }

    pub(crate) fn linked(rpc: link::Link, request_timeout: Duration, edge: &Edge) -> Self {
        Self {
            rpc,
            request_timeout,
            edge: edge.clone(),
        }
    }
}
