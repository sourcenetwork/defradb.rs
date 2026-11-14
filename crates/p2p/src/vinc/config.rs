//! What a vinc endpoint is built from.

use std::net::SocketAddr;
use std::sync::Arc;

use rapidhash::RapidHashSet;
use zeroize::Zeroizing;

/// Who may open a connection to this node. Revocation is separate and bars a
/// peer in both directions whatever this says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum VincAllowlistConfig {
    /// Accept a connection from any peer.
    #[default]
    AcceptAll,
    /// Accept a connection only from the listed peer ids.
    Explicit(RapidHashSet<String>),
}

/// A vinc endpoint's configuration.
#[derive(Clone)]
pub struct VincEndpointConfig {
    /// The Ed25519 seed, the one the node shares with libp2p and iroh, so a
    /// node keeps its peer id whichever transport it runs.
    pub seed: Zeroizing<[u8; 32]>,
    /// The UDP address to bind; port 0 picks one.
    pub bind: SocketAddr,
    pub allowlist: VincAllowlistConfig,
    /// The node's DID identity, which answers identity challenges.
    pub node_identity: Option<Arc<identity::RawIdentity>>,
    /// The most sessions at once. vinc reserves about 360 KiB of tables per
    /// session slot when it starts, so this is sized to the node, not left at
    /// vinc's default of 1024.
    pub max_connections: usize,
    /// The hubs topic broadcast runs through, as `<peer id>@<udp address>`,
    /// at most four. vinc topics travel through a hub: with none, a node's
    /// subscriptions and publishes reach no one.
    pub hubs: Vec<String>,
}

impl VincEndpointConfig {
    /// The default session table: enough for a small mesh at about 23 MiB.
    pub const DEFAULT_MAX_CONNECTIONS: usize = 64;

    pub fn new(seed: Zeroizing<[u8; 32]>, bind: SocketAddr) -> Self {
        Self {
            seed,
            bind,
            allowlist: VincAllowlistConfig::AcceptAll,
            node_identity: None,
            max_connections: Self::DEFAULT_MAX_CONNECTIONS,
            hubs: Vec::new(),
        }
    }
}

impl std::fmt::Debug for VincEndpointConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VincEndpointConfig")
            .field("bind", &self.bind)
            .field("allowlist", &self.allowlist)
            .field("max_connections", &self.max_connections)
            .field("hubs", &self.hubs)
            .finish_non_exhaustive()
    }
}
