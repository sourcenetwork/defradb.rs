//! The browser as a P2P peer.
//!
//! A browser owns no UDP socket, so its iroh endpoint reaches every peer
//! through a relay over WebSocket. Above the transport it is the same peer a
//! native node runs: the same coordinator, replication stack, and document ACP.

mod config;
mod runtime;

pub(crate) use config::P2PConfig;
pub(crate) use runtime::P2PRuntime;
