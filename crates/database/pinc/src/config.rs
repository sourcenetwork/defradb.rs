//! What a pinc endpoint is built from.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use rapidhash::RapidHashSet;
use zeroize::Zeroizing;

use super::stream_protocols::StreamRegistration;

/// Who may open a connection to this node. Revocation is separate and bars a
/// peer in both directions whatever this says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PincAllowlistConfig {
    /// Accept a connection from any peer.
    #[default]
    AcceptAll,
    /// Accept a connection only from the listed peer ids.
    Explicit(RapidHashSet<String>),
}

/// A pinc endpoint's configuration.
#[derive(Clone)]
pub struct PincEndpointConfig {
    /// The Ed25519 seed the node's peer id comes from.
    pub seed: Zeroizing<[u8; 32]>,
    /// The UDP address to bind; port 0 picks one. A wildcard IP (`0.0.0.0`,
    /// `[::]`) listens on every interface and lists each interface's
    /// address, never loopback.
    pub bind: SocketAddr,
    /// Addresses peers reach this node at that are on none of its
    /// interfaces, such as a public address in front of a NAT or a
    /// forwarded port. The node lists them first.
    pub public_addrs: Vec<SocketAddr>,
    pub allowlist: PincAllowlistConfig,
    /// The node's DID identity, which answers identity challenges.
    pub node_identity: Option<Arc<identity::RawIdentity>>,
    /// The most sessions at once. pinc reserves about 360 KiB of tables per
    /// session slot when it starts, so this is sized to the node, not left at
    /// pinc's default of 1024.
    pub max_connections: usize,
    /// The most tasks the endpoint runs at once: its own loops, one per
    /// connection, stream, dial and request. A command that needs one more
    /// is answered `the pinc endpoint is busy`.
    pub max_tasks: usize,
    /// How long a block sync (a CAR fetch) waits for a provider's blocks
    /// once its request is written.
    pub block_sync_timeout: Duration,
    /// The hubs topic broadcast runs through, as `<peer id>@<udp address>`,
    /// at most four. pinc topics travel through a hub: with none, a node's
    /// subscriptions and publishes reach no one. A hub that runs the relay
    /// role also carries this node's sessions to peers it cannot reach
    /// directly.
    pub hubs: Vec<String>,
    /// The embedder's stream protocols, each with its handler.
    pub stream_protocols: Vec<StreamRegistration>,
}

impl PincEndpointConfig {
    /// The default session table: enough for a small mesh at about 23 MiB.
    pub const DEFAULT_MAX_CONNECTIONS: usize = 64;
    /// The default task table.
    pub const DEFAULT_MAX_TASKS: usize = 8192;
    /// The default block sync timeout, Go's `net.p2pblocksynctimeout`.
    pub const DEFAULT_BLOCK_SYNC_TIMEOUT: Duration = Duration::from_secs(30);

    pub fn new(seed: Zeroizing<[u8; 32]>, bind: SocketAddr) -> Self {
        Self {
            seed,
            bind,
            public_addrs: Vec::new(),
            allowlist: PincAllowlistConfig::AcceptAll,
            node_identity: None,
            max_connections: Self::DEFAULT_MAX_CONNECTIONS,
            max_tasks: Self::DEFAULT_MAX_TASKS,
            block_sync_timeout: Self::DEFAULT_BLOCK_SYNC_TIMEOUT,
            hubs: Vec::new(),
            stream_protocols: Vec::new(),
        }
    }
}

impl std::fmt::Debug for PincEndpointConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PincEndpointConfig")
            .field("bind", &self.bind)
            .field("public_addrs", &self.public_addrs)
            .field("allowlist", &self.allowlist)
            .field("max_connections", &self.max_connections)
            .field("max_tasks", &self.max_tasks)
            .field("block_sync_timeout", &self.block_sync_timeout)
            .field("hubs", &self.hubs)
            .field("stream_protocols", &self.stream_protocols.len())
            .finish_non_exhaustive()
    }
}
