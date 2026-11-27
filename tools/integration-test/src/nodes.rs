//! The nodes a test runs, started, stopped and restarted the way Go's harness
//! does: each through the test's client type, on the core's node builder and
//! its shutdown.

use std::any::Any;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use cli::commands::client::http_client::HttpClient;
use defra_node::{HttpConfig, Node};
use events::{EventName, Message, MessageData};
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};
use regolith::sync::Atom;
use tempfile::TempDir;

use crate::actions::shared::KeyType;
use crate::client_type::{go_binary, network_edge, ClientType};
use crate::crash::NodeProcess;
use crate::go_node::{GoNode, GoOptions};

mod action_events;

pub use action_events::ActionEvents;

/// How a node is started, kept so a restart starts it the same way.
#[derive(Clone, Default)]
pub struct NodeSpec {
    pub network: bool,
    /// The node's data stays here across a restart; `None` keeps it in
    /// memory, where a restart is a no-op, as in Go's harness.
    pub dir: Option<Arc<TempDir>>,
    /// The P2P addresses a restarted node listens on again.
    pub listen: Vec<String>,
    /// A stopped Go node's `listen` ports, held so nothing else binds them
    /// before it starts again.
    pub reserved: Vec<Arc<crate::reserved_port::ReservedPort>>,
    /// Start with node access control enabled, as Go's `Start { EnableNAC }`.
    pub enable_nac: bool,
    /// The client identity seed starting the node, which owns its NAC when
    /// it enables it.
    pub nac_owner: Option<u64>,
    /// The node's admission limits, if not the defaults.
    pub admission: Option<crate::actions::AdmissionSpec>,
    /// The node's connection manager, if not the default.
    pub connection_manager: Option<crate::actions::ConnectionManagerSpec>,
    /// The `resolv.conf` text the node's libp2p DNS layers read in place of
    /// the host's file.
    pub resolv_conf: Option<String>,
    /// The node's network is iroh rather than libp2p.
    pub iroh_network: bool,
    /// The node was configured from a Go-written `config.yaml` through the
    /// Rust CLI's loader, so it is a Rust node under every client.
    pub go_config: bool,
    /// The node starts on a directory holding Go DefraDB's Badger store,
    /// which only a Rust node's refusal is checked against.
    pub go_datastore: bool,
    /// The node's own identity, Go's `NodeIdentity(seed)`; the node signs as it.
    pub node_identity: Option<u64>,
    /// The node takes `node_identity` from the process's signing registry,
    /// registered there before it starts, as an FFI host does.
    pub registered_identity: bool,
    /// The node's index in its test, which picks the key Go's harness signs
    /// with when the node has no `node_identity`.
    pub go_index: Option<u64>,
    /// The key type the node signs with, when it signs.
    pub signing: Option<crate::actions::shared::KeyType>,
    /// How long an idle explicit transaction lives and the sweep interval.
    pub txn_ttl: Option<(Duration, Duration)>,
    /// How many times the node re-runs a transaction that lost a race, if
    /// not the default.
    pub max_txn_retries: Option<u32>,
    /// The bytes the node's vector indexes may keep in memory between
    /// searches, if not the default.
    pub vector_memory_budget: Option<u64>,
    /// The node's own searchable-encryption key, as Go's harness gives each
    /// node one under `EnableSearchableEncryption`.
    pub se_key: Option<[u8; 32]>,
    /// The gate the node's index backfills pass before each batch; it lives
    /// across a restart, as Go's package-level hook does.
    pub index_gate: Option<Arc<crate::index_gate::TestIndexGate>>,
    /// Documents an index backfill may index in one transaction.
    pub backfill_batch_docs: Option<usize>,
    /// The gate explicit commits pass before they begin, for a real-node
    /// scenario that holds a commit past its HTTP bound.
    pub commit_gate: Option<Arc<crate::commit_gate::TestCommitGate>>,
    /// The HTTP server's bound on answering a commit; `None` leaves it off.
    pub commit_timeout: Option<Duration>,
    /// An in-process iroh node serving these stream protocols, in place of
    /// the libp2p one `network` starts.
    pub iroh: Option<crate::actions::stream::IrohSpec>,
    /// A Rust node in a process of its own, which a test can kill
    /// (`crate::crash`), under the HTTP and CLI clients.
    pub own_process: bool,
}

/// A running node: the node itself for the in-process client, or the URL the
/// HTTP and CLI clients reach it at, and what must outlive it.
pub struct TestNode {
    pub node: Option<Arc<Node>>,
    pub url: Option<String>,
    pub spec: NodeSpec,
    /// The Rust node behind any client, for its events and its shutdown.
    embedded: Option<Arc<Node>>,
    /// The process of a Rust node that runs in one of its own.
    process: Option<Arc<NodeProcess>>,
    _held: Vec<Arc<dyn Any + Send + Sync>>,
    events: Atom<Option<Arc<Events>>>,
    /// The node's collection action events, once a `ListActions { listen }`
    /// started recording them.
    actions: Atom<Option<Arc<ActionEvents>>>,
}

impl TestNode {
    /// Start the in-process iroh node `spec.iroh` describes.
    pub fn start_iroh(spec: NodeSpec) -> Result<Self, String> {
        let iroh = spec.iroh.clone().unwrap_or_default();
        let node = crate::actions::stream::build(&iroh)?;
        Ok(Self::new(
            Some(node.clone()),
            None,
            spec,
            Some(node),
            Vec::new(),
        ))
    }

    pub fn start(client_type: ClientType, spec: NodeSpec) -> Self {
        Self::try_start(client_type, spec).unwrap_or_else(|error| panic!("start node: {error}"))
    }

    /// [`TestNode::start`], failing with the node builder's error.
    pub fn try_start(client_type: ClientType, spec: NodeSpec) -> Result<Self, String> {
        if spec.iroh.is_some() {
            return Self::start_iroh(spec);
        }
        if spec.own_process && matches!(client_type, ClientType::RustHttp | ClientType::Cli) {
            return Self::start_process(spec);
        }
        let listen = match spec.listen.first() {
            Some(address) => address.clone(),
            None => "/ip4/127.0.0.1/tcp/0".to_owned(),
        };
        // Admission limits, a connection manager, iroh, a resolv.conf and a
        // loaded Go config are a Rust node's options: under go-http the node
        // is a Rust node reached over HTTP, as rust-http does.
        let client_type = match client_type {
            ClientType::GoHttp
                if spec.admission.is_some()
                    || spec.connection_manager.is_some()
                    || spec.iroh_network
                    || spec.resolv_conf.is_some()
                    || spec.go_config
                    || spec.go_datastore =>
            {
                ClientType::RustHttp
            }
            other => other,
        };
        match client_type {
            ClientType::InProcess => {
                let node = Arc::new(build(&spec, &listen, None)?);
                Ok(Self::new(
                    Some(node.clone()),
                    None,
                    spec,
                    Some(node),
                    Vec::new(),
                ))
            }
            ClientType::RustHttp | ClientType::Cli => {
                if client_type == ClientType::Cli {
                    let binary = crate::rust_binary();
                    assert!(
                        binary.is_file(),
                        "DEFRA_CLIENT=cli runs {}: build it with `cargo build -p cli`, or set \
                         DEFRA_RUST_BINARY",
                        binary.display()
                    );
                }
                let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind port");
                let url = format!("http://{}", listener.local_addr().expect("local address"));
                let mut config = HttpConfig::with_listener(listener)
                    .expect("http config")
                    .with_dev_mode(true)
                    .with_collection_management(true);
                if let Some((ttl, tick)) = spec.txn_ttl {
                    config = config
                        .with_transaction_idle_timeout(ttl)
                        .with_transaction_cleanup_interval(tick);
                }
                if let Some(bound) = spec.commit_timeout {
                    config = config.with_commit_timeout(bound);
                }
                let node = Arc::new(build(&spec, &listen, Some(config))?);
                Ok(Self::new(None, Some(url), spec, Some(node), Vec::new()))
            }
            ClientType::GoHttp => {
                let binary = go_binary().unwrap_or_else(|error| panic!("{error}"));
                let options = GoOptions {
                    p2p: spec.network.then_some(listen),
                    dir: spec.dir.as_ref().map(|dir| dir.path().to_owned()),
                    nac: spec.enable_nac,
                    identity: spec
                        .nac_owner
                        .map(crate::scenario::identity::private_key_hex),
                    node_key: go_node_key(&spec),
                    signing: spec.signing.is_some(),
                    se_key: spec.se_key,
                };
                let node = GoNode::start(&binary, &options).map_err(|error| error.to_string())?;
                let url = node.api_url().to_owned();
                Ok(Self::new(None, Some(url), spec, None, vec![Arc::new(node)]))
            }
        }
    }

    /// Start a Rust node in a process of its own, on `spec.dir`.
    fn start_process(spec: NodeSpec) -> Result<Self, String> {
        let dir = spec
            .dir
            .clone()
            .ok_or("a node in its own process keeps its data in a directory")?;
        let process = Arc::new(NodeProcess::start(dir.path())?);
        let mut node = Self::new(None, Some(process.url().to_owned()), spec, None, Vec::new());
        node.process = Some(process);
        Ok(node)
    }

    fn new(
        node: Option<Arc<Node>>,
        url: Option<String>,
        spec: NodeSpec,
        embedded: Option<Arc<Node>>,
        held: Vec<Arc<dyn Any + Send + Sync>>,
    ) -> Self {
        Self {
            node,
            url,
            spec,
            embedded,
            process: None,
            _held: held,
            events: Atom::new(None),
            actions: Atom::new(None),
        }
    }

    /// The node runs Go's defradb in another process.
    pub fn is_go(&self) -> bool {
        self.embedded.is_none() && self.process.is_none()
    }

    /// The process of a Rust node that runs in one of its own.
    pub fn process(&self) -> Option<&Arc<NodeProcess>> {
        self.process.as_ref()
    }

    /// The Rust node behind any client; `None` for a Go node.
    pub fn core(&self) -> Option<Arc<Node>> {
        self.embedded.clone()
    }

    /// Stop the node through the core's shutdown; a Go node is stopped when
    /// it is dropped.
    pub async fn stop(&self) {
        if let Some(node) = &self.embedded {
            node.shutdown().await.unwrap();
        }
        if let Some(process) = &self.process {
            process
                .stop()
                .unwrap_or_else(|error| panic!("stop the node: {error}"));
        }
    }

    /// Start recording the node's update, merge, topic and peer
    /// disconnection events.
    pub fn watch(&self) -> Result<(), String> {
        let node = self
            .embedded
            .as_ref()
            .ok_or("events of a node in another process")?;
        let subscription = node.event_bus().subscribe(&[
            EventName::Update,
            EventName::MergeComplete,
            EventName::TopicPeerEvent,
            EventName::PeerConnection,
            EventName::P2PGossipHeadDropped,
            EventName::SEArtifactReceived,
        ]);
        self.events
            .store(Some(Arc::new(Events::new(subscription.into_receiver()))));
        Ok(())
    }

    /// Start recording the node's collection action events, as Go's
    /// `action.events.start` does; recording again keeps the first record.
    pub fn listen_actions(&self) -> Result<Arc<ActionEvents>, String> {
        if let Some(actions) = self.actions.load_clone() {
            return Ok(actions);
        }
        let node = self
            .embedded
            .as_ref()
            .ok_or("action events of a node in another process")?;
        let subscription = node.event_bus().subscribe(&[EventName::ActionExecution]);
        let actions = Arc::new(ActionEvents::new(subscription.into_receiver()));
        self.actions.store(Some(actions.clone()));
        Ok(actions)
    }

    /// The node's recorded action events, when recording started.
    pub fn action_events(&self) -> Option<Arc<ActionEvents>> {
        self.actions.load_clone()
    }

    pub fn events(&self) -> Result<Arc<Events>, String> {
        self.events
            .load_clone()
            .ok_or_else(|| "no events recorded: WaitForPeersEvents { start } did not run".into())
    }

    /// Wait for the next `ReplicatorCompleted` after `action`, up to
    /// `deadline`, as Go's harness does after adding a replicator.
    pub fn replicator_completed<F, T, E>(&self, action: F, deadline: Duration) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let Some(node) = &self.embedded else {
            return action();
        };
        let completed = node
            .event_bus()
            .subscribe(&[EventName::ReplicatorCompleted])
            .into_receiver();
        let done = action()?;
        drop(workers::block_on(workers::timeout(
            deadline,
            completed.recv(),
        )));
        Ok(done)
    }

    /// The Rust node behind whichever client the test runs through, for the
    /// test's own view of it; `None` for a Go node.
    pub fn embedded(&self) -> Option<&Arc<Node>> {
        self.embedded.as_ref()
    }

    /// The node's P2P addresses, as Go's `PeerInfo` lists them: the test's
    /// own view of the node, whichever client the test runs through.
    pub fn peer_info(&self) -> Result<Vec<String>, String> {
        // The node's NAC owner may read it once NAC is on.
        let owner = self.spec.nac_owner;
        match (&self.embedded, &self.url) {
            (Some(node), _) => {
                let scoped = match owner {
                    Some(seed) => node.with_identity(
                        ::identity::Did::new(crate::scenario::identity::did(seed))
                            .map_err(|e| e.to_string())?,
                    ),
                    None => node.anonymous(),
                };
                workers::block_on(scoped.peer_info())
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())
            }
            (None, Some(url)) => {
                let token = owner.map(|seed| {
                    cli::commands::client::generate_auth_token(
                        &crate::scenario::identity::private_key_hex(seed),
                        url,
                    )
                    .expect("a token for a test identity")
                });
                let client = HttpClient::new(url, &network_edge())
                    .map_err(|e| e.to_string())?
                    .with_auth_token(token);
                workers::block_on(network_edge().spawn(async move { client.peer_info().await }))
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())
            }
            (None, None) => Ok(Vec::new()),
        }
    }
}

fn build(spec: &NodeSpec, listen: &str, http: Option<HttpConfig>) -> Result<Node, String> {
    // Every Rust node runs in this process. Go's test binary encrypts with its
    // fixed test nonce and test key, so expected `_commits` ciphertexts only
    // match with both, as the FFI's `defra_init` sets them for Go's suite.
    crypto::encryption::nonce::set_deterministic_nonce(true);
    defra_core::encryption::set_deterministic_encryption_key(true);
    let mut builder = Node::builder();
    if let Some(dir) = &spec.dir {
        builder = builder.data_path(dir.path());
    }
    if spec.network && spec.iroh_network {
        builder = builder.with_p2p(defra_node::P2PConfig {
            bind_addr: Some(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
            relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
            discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
            ..Default::default()
        });
    } else if spec.network {
        builder = builder.with_libp2p(listen);
    }
    if spec.network
        && !spec.iroh_network
        && (spec.admission.is_some() || spec.connection_manager.is_some())
    {
        let mut libp2p = defra_node::Libp2pConfig {
            listen_addrs: vec![listen.to_owned()],
            ..Default::default()
        };
        if let Some(limits) = &spec.admission {
            libp2p.sync.max_pending_dags = limits.max_pending_dags;
            libp2p.sync.rate_limit_burst = limits.rate_limit_burst;
            libp2p.sync.rate_limit_rate = limits.rate_limit_rate;
            if let Some(bytes) = limits.deferral_peer_bytes {
                libp2p.sync.gossip_deferral_peer_bytes = bytes;
            }
        }
        if let Some(manager) = &spec.connection_manager {
            libp2p.connection_manager_low_water = manager.low_water;
            libp2p.connection_manager_high_water = manager.high_water;
            libp2p.connection_manager_grace_period = Duration::from_millis(manager.grace_period_ms);
        }
        builder = builder.with_transport(defra_node::TransportConfig::Libp2p(libp2p));
    }
    #[cfg(unix)]
    if let Some(text) = &spec.resolv_conf {
        builder = builder.with_resolver_source(p2p::ResolverSource::ResolvConf(text.clone()));
    }
    // A scenario that picks iroh tests iroh itself (its gossip size limit),
    // so DEFRA_TRANSPORT=pinc replaces only the default transport. It also
    // replaces libp2p, so a `resolv_conf` scenario runs on pinc here and its
    // libp2p DNS fallback goes untested under DEFRA_TRANSPORT=pinc.
    if spec.network
        && !spec.iroh_network
        && crate::transport_choice::TransportChoice::from_env().is_pinc()
    {
        builder = pinc_transport(builder, spec, listen);
    }
    if let Some(config) = http {
        builder = builder.with_http(config);
    } else if let Some((ttl, tick)) = spec.txn_ttl {
        builder = builder.with_transaction_idle_timeout(ttl, tick);
    }
    if let Some(retries) = spec.max_txn_retries {
        builder = builder.with_max_txn_retries(retries);
    }
    if let Some(bytes) = spec.vector_memory_budget {
        builder = builder.with_vector_memory_budget(bytes);
    }
    if let Some(gate) = &spec.index_gate {
        builder = builder.with_index_build_gate(gate.clone());
    }
    if let Some(docs) = spec.backfill_batch_docs {
        builder = builder.with_backfill_batch_docs(docs);
    }
    if let Some(gate) = &spec.commit_gate {
        builder = builder.with_commit_gate(gate.clone());
    }
    if spec.registered_identity {
        builder = builder.with_node_identity_did(register_node_identity(spec)?);
    } else if spec.node_identity.is_some() || spec.signing.is_some() {
        builder = builder.with_signing_key(signing_key(spec));
    }
    // Go keeps the identity and the signing apart: a node with an identity
    // signs only when the scenario turns signing on.
    if spec.node_identity.is_some() && spec.signing.is_none() {
        builder = builder.with_identity_only();
    }
    if spec.enable_nac {
        builder = match &spec.nac_owner {
            Some(seed) => builder.with_node_acp_owner(crate::scenario::identity::did(*seed)),
            None => builder.with_node_acp_enabled(),
        };
    }
    let node = workers::block_on(builder.build())
        .expect("run builder")
        .map_err(|error| error.to_string())?;
    if let Some(key) = spec.se_key {
        let system = node
            .p2p_system()
            .ok_or("searchable encryption needs a node with P2P")?;
        system.set_replicator_push_options(defra_node::ReplicatorPushOptions {
            se_encryption_key: Some(defra_node::Zeroizing::new(key.to_vec())),
            se_identity_pubkey: None,
        })?;
    }
    Ok(node)
}

/// Run `builder`'s node over pinc on loopback, `DEFRA_TRANSPORT=pinc`. A node
/// restarted on its old address binds the port that address names; its key
/// lives in its store, so a restart keeps its peer id.
#[cfg(feature = "pinc")]
fn pinc_transport(
    builder: defra_node::NodeBuilder,
    spec: &NodeSpec,
    listen: &str,
) -> defra_node::NodeBuilder {
    let bind = listen
        .split_once('@')
        .and_then(|(_, addrs)| addrs.split(',').next())
        .and_then(|addr| addr.parse().ok())
        .unwrap_or_else(|| std::net::SocketAddr::from(([127, 0, 0, 1], 0)));
    let mut config = defra_node::PincConfig {
        bind,
        peer_key: defra_node::PeerKeySource::Store,
        max_connections: 16,
        hubs: vec![PINC_HUB.clone()],
        ..Default::default()
    };
    if let Some(limits) = &spec.admission {
        config.sync.max_pending_dags = limits.max_pending_dags;
        config.sync.rate_limit_burst = limits.rate_limit_burst;
        config.sync.rate_limit_rate = limits.rate_limit_rate;
        if let Some(bytes) = limits.deferral_peer_bytes {
            config.sync.gossip_deferral_peer_bytes = bytes;
        }
    }
    builder.with_pinc(config)
}

/// The pinc hub every node of this test process names, as a gossip mesh's
/// stand-in: pinc topics run through a hub.
#[cfg(feature = "pinc")]
static PINC_HUB: regolith::sync::Lazy<String> = regolith::sync::Lazy::new(|| {
    let edge = edge::testing::edge();
    let bind = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    let (ticket, task) = workers::block_on(
        edge.clone()
            .spawn(async move { defra_node::spawn_pinc_hub([0x48; 32], bind, edge).await }),
    )
    .expect("the test thread waits")
    .expect("the hub task runs")
    .expect("the pinc hub binds");
    // ponytail: the hub lives as long as the process; nothing stops it.
    std::mem::forget(task);
    ticket
});

#[cfg(not(feature = "pinc"))]
fn pinc_transport(
    _builder: defra_node::NodeBuilder,
    _spec: &NodeSpec,
    _listen: &str,
) -> defra_node::NodeBuilder {
    panic!("DEFRA_TRANSPORT=pinc needs integration-test built with --features pinc")
}

/// The key a node signs with: its node identity's, or, for a node that only
/// signs, a key of its own that no test identity shares.
fn signing_key(spec: &NodeSpec) -> defra_node::SigningKey {
    // Go's node `i` signs as `NodeIdentity(i)` unless told otherwise.
    let seed = spec.node_identity.or(spec.go_index).unwrap_or(u64::MAX);
    let key = crate::scenario::identity::node_private_key(seed).to_vec();
    match spec.signing.unwrap_or_default() {
        KeyType::Secp256k1 => defra_node::SigningKey::Secp256k1(key),
        // An Ed25519 key is stored as its seed and its public key.
        KeyType::Ed25519 => defra_node::SigningKey::Ed25519(
            crypto::keys::ed25519::ed25519_key_from_seed(&key)
                .expect("any 32 bytes are an Ed25519 seed"),
        ),
    }
}

/// Registers the key a node started as `spec` signs with in the process's
/// signing registry, as an FFI host registers an identity before
/// `defra_node_new`, and answers its DID. The registration is pinned for the
/// process, so a restart registers the same signer again.
fn register_node_identity(spec: &NodeSpec) -> Result<String, String> {
    use defra_core::signing::{SigningConfig, SigningKeyType};
    let seed = spec.node_identity.or(spec.go_index).unwrap_or(u64::MAX);
    let (key_type, signing_key_type) = match spec.signing.unwrap_or_default() {
        KeyType::Secp256k1 => (::identity::KeyType::Secp256k1, SigningKeyType::Secp256k1),
        KeyType::Ed25519 => (::identity::KeyType::Ed25519, SigningKeyType::Ed25519),
    };
    let raw = ::identity::RawIdentity::from_bytes(
        key_type,
        &crate::scenario::identity::node_private_key(seed),
    )
    .map_err(|error| error.to_string())?;
    let did = ::identity::Identity::did(&raw)
        .map_err(|error| error.to_string())?
        .to_string();
    let public_key_bytes = raw.public_key_bytes();
    defra_core::signing::store_identity(
        &did,
        SigningConfig {
            key_type: signing_key_type,
            private_key_bytes: SigningConfig::private_key_bytes_from_vec(raw.private_key_bytes()),
            public_key_hex: hex::encode(&public_key_bytes),
            public_key_bytes,
            remote_signer: None,
            signing_authorization: None,
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(did)
}

/// The node identity a Go node started as `spec` loads from its keyring: the
/// key a Rust node started the same way signs with.
fn go_node_key(spec: &NodeSpec) -> Option<crate::go_node::NodeKey> {
    if spec.node_identity.is_none() && spec.signing.is_none() {
        return None;
    }
    let (key_type, raw) = match signing_key(spec) {
        defra_node::SigningKey::Secp256k1(key) => ("secp256k1", key),
        // Go's Ed25519 private key is the seed, then the public key.
        defra_node::SigningKey::Ed25519(pair) => ("ed25519", pair),
        _ => unreachable!("a test node signs with secp256k1 or Ed25519"),
    };
    Some(crate::go_node::NodeKey { key_type, raw })
}

/// The public key (hex) and key type a node started as `spec` signs blocks
/// with, as their signature headers name it; `None` when it does not sign.
pub fn signer_public_key(spec: &NodeSpec) -> Option<(String, KeyType)> {
    // An identity alone signs nothing.
    spec.signing?;
    let public = |key_type, key: Vec<u8>| {
        ::identity::RawIdentity::from_bytes(key_type, &key)
            .expect("a valid key")
            .public_key_bytes()
    };
    let public = match signing_key(spec) {
        defra_node::SigningKey::Secp256k1(key) => public(::identity::KeyType::Secp256k1, key),
        // The seed, then the public key.
        defra_node::SigningKey::Ed25519(pair) => pair[32..].to_vec(),
        _ => unreachable!("a test node signs with secp256k1 or Ed25519"),
    };
    Some((hex::encode(public), spec.signing.unwrap_or_default()))
}

/// The named field links of the block `bytes` encode, as Go's harness reads
/// `Block.Links`; none when the bytes are not a block.
fn field_links(bytes: &[u8]) -> RapidHashMap<String, String> {
    defra_core::block::Block::from_dag_cbor(bytes)
        .ok()
        .and_then(|block| block.links)
        .into_iter()
        .flatten()
        .filter(|link| link.name != "_head")
        .map(|link| (link.name, link.link.to_string()))
        .collect()
}

/// What a node's events said, read in order by the waits.
#[derive(Clone, Default)]
struct EventLog {
    /// Local updates not yet awaited, oldest first: (document, head).
    updates: VecDeque<(String, String)>,
    /// Every (document, head) a merge completed.
    merged: RapidHashSet<(String, String)>,
    /// The newest head seen for each document.
    heads: RapidHashMap<String, String>,
    /// Topic events not yet awaited: (topic, peer, JOINED or LEFT).
    topics: VecDeque<(String, String, String)>,
    /// Peers whose last connection closed, not yet awaited, oldest first.
    disconnected: VecDeque<String>,
    /// Gossip heads the node dropped past a deferral budget, not yet awaited.
    gossip_dropped: VecDeque<events::P2PGossipHeadDroppedData>,
    /// Every document a searchable-encryption artifact arrived for.
    se_synced: RapidHashSet<String>,
    /// The field blocks each local update's head links to, by head: field
    /// name to block CID, as Go's harness records `FieldCIDs`.
    field_cids: RapidHashMap<String, RapidHashMap<String, String>>,
}

/// A node's recorded events.
pub struct Events {
    receiver: async_channel::Receiver<Message>,
    log: Atom<EventLog>,
}

impl Events {
    fn new(receiver: async_channel::Receiver<Message>) -> Self {
        Self {
            receiver,
            log: Atom::new(EventLog::default()),
        }
    }

    fn update<F>(&self, change: F)
    where
        F: FnOnce(&mut EventLog),
    {
        let mut log = self.log.load_clone();
        change(&mut log);
        self.log.store(log);
    }

    /// Receive one event within `deadline` and record it.
    async fn receive(&self, deadline: Duration) -> Result<(), String> {
        let message = match workers::timeout(deadline, self.receiver.recv()).await {
            Ok(Ok(message)) => message,
            Ok(Err(_)) => return Err("the node's event stream closed".into()),
            Err(_) => return Err(format!("no event within {deadline:?}")),
        };
        match message.data {
            MessageData::Update(update) if !update.is_relay => {
                let key = match update.doc_id.is_empty() {
                    true => update.collection_id,
                    false => update.doc_id,
                };
                let head = (key, update.cid.to_string());
                let fields = field_links(&update.block);
                self.update(|log| {
                    log.heads.insert(head.0.clone(), head.1.clone());
                    log.field_cids.insert(head.1.clone(), fields);
                    log.updates.push_back(head);
                });
            }
            MessageData::MergeComplete(merge) => {
                let key = match merge.doc_id.is_empty() {
                    true => merge.collection_id,
                    false => merge.doc_id,
                };
                let head = (key, merge.cid.to_string());
                self.update(|log| {
                    log.heads.insert(head.0.clone(), head.1.clone());
                    log.merged.insert(head);
                });
            }
            MessageData::TopicPeerEvent(event) => {
                self.update(|log| {
                    log.topics
                        .push_back((event.topic, event.peer_id, event.event_type))
                });
            }
            MessageData::PeerConnection(event) if !event.connected => {
                self.update(|log| log.disconnected.push_back(event.peer_id))
            }
            MessageData::P2PGossipHeadDropped(event) => {
                self.update(|log| log.gossip_dropped.push_back(event))
            }
            MessageData::SEArtifactReceived(event) => self.update(|log| {
                log.se_synced.insert(event.doc_id);
            }),
            _ => {}
        }
        Ok(())
    }

    /// The field blocks update head `cid` links to, by field name.
    pub fn field_cids(&self, cid: &str) -> RapidHashMap<String, String> {
        self.log
            .load_clone()
            .field_cids
            .get(cid)
            .cloned()
            .unwrap_or_default()
    }

    /// The heads of the next local updates, one for each of `keys` (a
    /// document ID, or a branchable collection's ID), in any order, as Go's
    /// harness reads them.
    pub async fn next_updates(
        &self,
        keys: &[&str],
        deadline: Duration,
    ) -> Result<Vec<String>, String> {
        let mut heads: RapidHashMap<String, String> = RapidHashMap::new();
        while heads.len() < keys.len() {
            let mut log = self.log.load_clone();
            let Some((key, head)) = log.updates.pop_front() else {
                self.receive(deadline).await?;
                continue;
            };
            self.log.store(log);
            if !keys.contains(&key.as_str()) || heads.contains_key(&key) {
                return Err(format!("unexpected update to {key}; expected {keys:?}"));
            }
            heads.insert(key, head);
        }
        Ok(keys.iter().map(|key| heads[*key].clone()).collect())
    }

    /// Wait until a searchable-encryption artifact arrived for each of
    /// `doc_ids`, as Go's `WaitForSESync` waits for `SEArtifactReceived`.
    pub async fn se_synced(&self, doc_ids: &[String], deadline: Duration) -> Result<(), String> {
        loop {
            let log = self.log.load_clone();
            let missing: Vec<&String> = doc_ids
                .iter()
                .filter(|id| !log.se_synced.contains(*id))
                .collect();
            if missing.is_empty() {
                return Ok(());
            }
            self.receive(deadline)
                .await
                .map_err(|reason| format!("no SE artifact for {missing:?}: {reason}"))?;
        }
    }

    /// Wait until `doc_id` has merged `head`, or already holds it as its
    /// newest head.
    pub async fn merged(&self, doc_id: &str, head: &str, deadline: Duration) -> Result<(), String> {
        let key = (doc_id.to_owned(), head.to_owned());
        loop {
            let log = self.log.load_clone();
            if log.merged.contains(&key) || log.heads.get(doc_id).map(String::as_str) == Some(head)
            {
                return Ok(());
            }
            self.receive(deadline)
                .await
                .map_err(|reason| format!("{doc_id} did not merge {head}: {reason}"))?;
        }
    }

    /// The next `count` gossip heads the node dropped past a deferral budget.
    pub async fn gossip_dropped(
        &self,
        count: usize,
        deadline: Duration,
    ) -> Result<Vec<events::P2PGossipHeadDroppedData>, String> {
        let end = std::time::Instant::now() + deadline;
        let mut dropped = Vec::with_capacity(count);
        while dropped.len() < count {
            let mut log = self.log.load_clone();
            if let Some(event) = log.gossip_dropped.pop_front() {
                self.log.store(log);
                dropped.push(event);
                continue;
            }
            let left = end.saturating_duration_since(std::time::Instant::now());
            self.receive(left).await.map_err(|reason| {
                format!(
                    "{} of {count} dropped gossip heads: {reason}",
                    dropped.len()
                )
            })?;
        }
        Ok(dropped)
    }

    /// Wait until every peer in `wanted` has lost its last connection to the
    /// node, consuming the disconnection events before it.
    pub async fn disconnected(
        &self,
        mut wanted: RapidHashSet<String>,
        deadline: Duration,
    ) -> Result<(), String> {
        let end = std::time::Instant::now() + deadline;
        loop {
            let mut log = self.log.load_clone();
            while let Some(peer) = log.disconnected.pop_front() {
                wanted.remove(&peer);
            }
            self.log.store(log);
            if wanted.is_empty() {
                return Ok(());
            }
            let left = end.saturating_duration_since(std::time::Instant::now());
            self.receive(left)
                .await
                .map_err(|reason| format!("missing disconnections {wanted:?}: {reason}"))?;
        }
    }

    /// Wait until every (topic, peer) in `wanted` has an event of `kind`,
    /// consuming the topic events before it.
    pub async fn topics(
        &self,
        mut wanted: RapidHashSet<(String, String)>,
        kind: &str,
        deadline: Duration,
    ) -> Result<(), String> {
        let end = std::time::Instant::now() + deadline;
        loop {
            let mut log = self.log.load_clone();
            while let Some((topic, peer, event)) = log.topics.pop_front() {
                if event == kind {
                    wanted.remove(&(topic, peer));
                }
            }
            self.log.store(log);
            if wanted.is_empty() {
                return Ok(());
            }
            let left = end.saturating_duration_since(std::time::Instant::now());
            self.receive(left)
                .await
                .map_err(|reason| format!("missing {kind} events {wanted:?}: {reason}"))?;
        }
    }
}

/// The peer ID of an address: the end of a libp2p `/p2p/<ID>` address, or
/// the start of a pinc `<ID>@<ip:port>` one.
pub fn peer_id(address: &str) -> &str {
    let id = address.rsplit("/p2p/").next().unwrap_or(address);
    id.split('@').next().unwrap_or(id)
}
