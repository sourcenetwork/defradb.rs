//! HTTP, TLS and gRPC on nuclei sockets.
//!
//! - [`rt`]: hyper's runtime hooks on nuclei: an I/O adapter, an executor
//!   and a timer.
//! - [`tls`]: rustls over nuclei streams, on the ring provider.
//! - [`server`]: the accept loop that serves a tower service over HTTP/1
//!   and HTTP/2, plain or over TLS.
//! - [`http`]: the HTTP client: pooled connections, HTTP/2 by ALPN, TLS
//!   with the system's root certificates, the system's proxies, redirects.
//! - [`timeout`]: a time limit on each request of a tower service.
//! - [`grpc`]: the HTTP/2 channel tonic's generated clients run on.
//! - [`connect`]: a client connection for a protocol spoken over a socket
//!   directly, such as a WebSocket.
//! - `libp2p` (feature `libp2p`): libp2p's TCP transport on nuclei sockets,
//!   and a swarm on it.
//!
//! Every connection is a nuclei socket. Tasks are spawned on the nuclei
//! runtime the caller runs on; nothing here starts a runtime or needs a
//! tokio one.

#![cfg(not(target_arch = "wasm32"))]

mod connect;
pub mod grpc;
pub mod http;
#[cfg(feature = "libp2p")]
pub mod libp2p;
pub mod rt;
pub mod server;
mod stream;
pub mod timeout;
pub mod tls;

pub use connect::connect;
pub use stream::ClientStream;

/// An error of any kind a connector or a body may give.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
