//! Iroh QUIC-native P2P transport for DefraDB.
//!
//! This module provides an alternative to the libp2p transport, using iroh's
//! QUIC-based networking stack. It is feature-gated behind `iroh-transport`.
//!
//! # Architecture
//!
//! - `IrohTransport`: Thin `Clone + Send + Sync` facade implementing `P2PTransport`
//! - `IrohEndpoint`: Background tokio task owning all iroh state
//! - Communication via `IrohCommand` enum over mpsc channel

mod addr;
#[cfg(test)]
mod allowlist_tests;
mod command;
mod config;
mod endpoint;
mod endpoint_commands;
mod endpoint_config;
mod endpoint_rpc;
mod endpoint_streams;
mod gossip_heal;
#[cfg(test)]
mod mux_tests;
mod peer_map;
mod protocols;
#[cfg(feature = "iroh-relay-server")]
mod relay_server;
#[cfg(all(test, feature = "iroh-relay-server"))]
mod relay_server_tests;
mod secret_key;
mod task_registry;
mod transport;
#[cfg(test)]
mod two_stream_tests;

pub use addr::{
    best_shareable_public_addr, canonical_peer_id, endpoint_addr_from_parts,
    endpoint_ticket_string, format_public_listen_addrs, is_ticket_string, parse_canonical_peer_id,
    parse_public_peer_addr,
};
pub use config::{IrohAllowlistConfig, IrohDiscoveryConfig, IrohRelayModeConfig};
pub use endpoint::spawn_endpoint;
pub use endpoint_config::{AdmissionAuthority, IrohEndpointConfig};
pub use gossip_heal::GossipHealConfig;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use iroh::SecretKey;
#[cfg(feature = "iroh-relay-server")]
pub use relay_server::{IrohRelayServer, IrohRelayServerConfig, IrohRelayTlsConfig};
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use secret_key::load_or_generate_secret_key;
pub use secret_key::{generate_secret_key, secret_key_from_bytes};
pub use transport::IrohTransport;
