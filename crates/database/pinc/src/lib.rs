//! DefraDB's P2P transport: a node over a pinc endpoint.
//!
//! Built on pinc's async facade (`sourcenetwork/pinc`), native only. It
//! carries what DefraDB needs: identity from the shared Ed25519 seed, dial by
//! address or by a peer id seen before, every request service with its reply
//! on the request's own stream, CAR sync, urgency by service class, admission
//! and revocation, the service-record pointer, path info, `network_change`,
//! peer exchange (peers of a peer connect), and topic pubsub (gossip head
//! hints and the raw topics KMS uses) through the hubs the node names.
//!
//! # Shape
//!
//! - [`PincTransport`] is the `Clone + Send + Sync` facade the engine holds.
//!   It reaches the endpoint over one bounded command queue.
//! - The endpoint task owns the pinc endpoint, which is `!Send`: it runs on
//!   one edge thread ([`edge::Edge::spawn_pinned`]) over a nuclei UDP socket,
//!   with every connection and stream task local to that thread. It never
//!   enters `edge::compat`, and nothing here touches tokio.
//! - An inbound request hands the engine a [`PincResponseToken`]: a `Send`
//!   id the endpoint maps to the request's `!Send` send stream. The bridge is
//!   modeled in `proofs/tla/PincReplyBridge.tla` and its table proved in
//!   `proofs/lean/PincReplyTable`.
//! - Topics run through pinc hubs ([`spawn_hub`] runs one); what reaches the
//!   engine is modeled in `proofs/tla/PincTopicDelivery.tla`.

#![cfg(not(target_arch = "wasm32"))]

mod addr;
mod admission;
mod command;
mod config;
mod directory;
mod endpoint;
mod exchange;
mod hub;
mod inbound_budget;
mod protocol_stream;
mod reply_table;
mod requests;
mod seed;
mod serve;
mod services;
mod stream_open;
mod stream_protocols;
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
pub use config::{PincAllowlistConfig, PincEndpointConfig};
pub use endpoint::spawn_endpoint;
pub use hub::{spawn_hub, HubConfig};
pub use inbound_budget::peer_share_bytes;
pub use protocol_stream::{ProtocolRecvStream, ProtocolSendStream, RESET_CANCELLED};
pub use seed::{generate_seed, load_or_generate_seed};
pub use serve::{RESET_BUSY, RESET_MESSAGE_TOO_LARGE, RESET_STALLED, RESET_UNKNOWN_SERVICE};
pub use services::{Priority, ALPN_MUX, SERVICE_POINTER_LEN};
pub use stream_protocols::{
    InboundStream, ProtocolRequest, StreamHandler, StreamProtocol, StreamRegistration,
    MAX_STREAM_DEADLINE, RESERVED_TAG_PREFIX,
};
pub use token::PincResponseToken;
pub use topics::PublishStats;
pub use transport::{PincPeerIdentityResolver, PincTransport};

/// The sync coordinator over the pinc transport.
pub type PincSyncCoordinator<B> = p2p::SyncCoordinator<B, PincTransport>;
