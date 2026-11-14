//! The vinc transport: a node over a vinc endpoint.
//!
//! A second Rust-to-Rust transport beside iroh, built on vinc's async facade
//! (`sourcenetwork/vinc`). It is opt-in (`vinc-transport`), native only, and
//! carries what DefraDB needs: identity from the shared Ed25519 seed, dial by
//! address or by a peer id seen before, every request service with its reply
//! on the request's own stream, CAR sync, urgency by service class, admission
//! and revocation, the service-record pointer, path info, `network_change`,
//! peer exchange (peers of a peer connect, as on libp2p and iroh),
//! and topic pubsub (gossip head hints and the raw topics KMS uses) through
//! the hubs the node names.
//!
//! # Shape
//!
//! - [`VincTransport`] is the `Clone + Send + Sync` facade the engine holds.
//!   It reaches the endpoint over one bounded command queue.
//! - The endpoint task owns the vinc endpoint, which is `!Send`: it runs on
//!   one edge thread ([`edge::Edge::spawn_pinned`]) over a nuclei UDP socket,
//!   with every connection and stream task local to that thread. It never
//!   enters `edge::compat`, and nothing here touches tokio.
//! - An inbound request hands the engine a [`VincResponseToken`]: a `Send`
//!   id the endpoint maps to the request's `!Send` send stream. The bridge is
//!   modeled in `proofs/tla/VincReplyBridge.tla` and its table proved in
//!   `proofs/lean/VincReplyTable`.
//! - Topics run through vinc hubs ([`spawn_hub`] runs one); what reaches the
//!   engine is modeled in `proofs/tla/VincTopicDelivery.tla`.
//!
//! The iroh backend's transport-neutral parts are copied here rather than
//! shared, so iroh's live code is untouched while vinc is evaluated.

mod addr;
mod admission;
mod command;
mod config;
mod directory;
mod endpoint;
mod exchange;
mod hub;
mod inbound_budget;
mod reply_table;
mod requests;
mod serve;
mod services;
mod token;
mod topics;
mod transport;
mod wire;

#[cfg(test)]
mod tests;

pub use addr::{
    best_shareable_public_addr, canonical_peer_id, format_public_listen_addrs,
    parse_canonical_peer_id, parse_public_peer_addr,
};
pub use admission::AdmissionAuthority;
pub use config::{VincAllowlistConfig, VincEndpointConfig};
pub use endpoint::spawn_endpoint;
pub use hub::spawn_hub;
pub use services::{ALPN_MUX, SERVICE_POINTER_LEN};
pub use token::VincResponseToken;
pub use transport::{VincPeerIdentityResolver, VincTransport};
