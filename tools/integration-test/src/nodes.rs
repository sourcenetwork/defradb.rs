//! The nodes a test runs, started, stopped and restarted the way Go's harness
//! does: each through the test's client type, on the core's node builder and
//! its shutdown.

use std::any::Any;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use cli::commands::client::http_client::HttpClient;
use defra_core::RequestContext;
use defra_node::{HttpConfig, Node};
use events::{EventName, Message, MessageData};
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};
use regolith::sync::Atom;
use tempfile::TempDir;

use crate::actions::shared::KeyType;
use crate::client_type::{go_binary, network_edge, ClientType};
use crate::go_node::{GoNode, GoOptions};

/// How a node is started, kept so a restart starts it the same way.
#[derive(Clone, Default)]
pub struct NodeSpec {
    pub network: bool,
    /// The node's data stays here across a restart; `None` keeps it in
    /// memory, where a restart is a no-op, as in Go's harness.
    pub dir: Option<Arc<TempDir>>,
    /// The P2P addresses a restarted node listens on again.
    pub listen: Vec<String>,
    /// Start with node access control enabled, as Go's `Start { EnableNAC }`.
    pub enable_nac: bool,
    /// The client identity seed starting the node, which owns its NAC when
    /// it enables it.
    pub nac_owner: Option<u64>,
    /// The node's admission limits, if not the defaults.
    pub admission: Option<crate::actions::AdmissionSpec>,
    /// The node's own identity, Go's `NodeIdentity(seed)`; the node signs as it.
    pub node_identity: Option<u64>,
    /// The key type the node signs with, when it signs.
    pub signing: Option<crate::actions::shared::KeyType>,
    /// How long an idle HTTP transaction lives and the sweep interval.
    pub txn_ttl: Option<(Duration, Duration)>,
    /// The node's own searchable-encryption key, as Go's harness gives each
    /// node one under `EnableSearchableEncryption`.
    pub se_key: Option<[u8; 32]>,
}

/// A running node: the node itself for the in-process client, or the URL the
/// HTTP and CLI clients reach it at, and what must outlive it.
pub struct TestNode {
    pub node: Option<Arc<Node>>,
    pub url: Option<String>,
    pub spec: NodeSpec,
    /// The Rust node behind any client, for its events and its shutdown.
    embedded: Option<Arc<Node>>,
    _held: Vec<Arc<dyn Any + Send + Sync>>,
    events: Atom<Option<Arc<Events>>>,
}

impl TestNode {
    pub fn start(client_type: ClientType, spec: NodeSpec) -> Self {
        Self::try_start(client_type, spec).unwrap_or_else(|error| panic!("start node: {error}"))
    }

    /// [`TestNode::start`], failing with the node builder's error.
    pub fn try_start(client_type: ClientType, spec: NodeSpec) -> Result<Self, String> {
        let listen = match spec.listen.first() {
            Some(address) => address.clone(),
            None => "/ip4/127.0.0.1/tcp/0".to_owned(),
        };
        // Admission limits are a Rust receiver's options: under go-http the
        // node is a Rust node reached over HTTP, as rust-http does.
        let client_type = match client_type {
            ClientType::GoHttp if spec.admission.is_some() => ClientType::RustHttp,
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
                    .with_dev_mode(true);
                if let Some((ttl, tick)) = spec.txn_ttl {
                    config = config
                        .with_transaction_idle_timeout(ttl)
                        .with_transaction_cleanup_interval(tick);
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
            _held: held,
            events: Atom::new(None),
        }
    }

    /// Stop the node through the core's shutdown; a Go node is stopped when
    /// it is dropped.
    pub async fn stop(&self) {
        if let Some(node) = &self.embedded {
            node.shutdown().await.unwrap();
        }
    }

    /// Start recording the node's update, merge and topic events.
    pub fn watch(&self) -> Result<(), String> {
        let node = self
            .embedded
            .as_ref()
            .ok_or("events of a node in another process")?;
        let subscription = node.event_bus().subscribe(&[
            EventName::Update,
            EventName::MergeComplete,
            EventName::TopicPeerEvent,
            EventName::P2PGossipHeadDropped,
            EventName::SEArtifactReceived,
        ]);
        self.events
            .store(Some(Arc::new(Events::new(subscription.into_receiver()))));
        Ok(())
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
        let context = RequestContext::ambient();
        match (&self.embedded, &self.url) {
            (Some(node), _) => workers::block_on(node.peer_info_as(&context))
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string()),
            (None, Some(url)) => {
                // The node's NAC owner may read it once NAC is on.
                let token = self.spec.nac_owner.map(|seed| {
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
    if spec.network {
        builder = builder.with_libp2p(listen);
    }
    if let (true, Some(limits)) = (spec.network, &spec.admission) {
        let mut sync = defra_node::SyncTuning {
            max_pending_dags: limits.max_pending_dags,
            rate_limit_burst: limits.rate_limit_burst,
            rate_limit_rate: limits.rate_limit_rate,
            ..Default::default()
        };
        if let Some(bytes) = limits.deferral_peer_bytes {
            sync.gossip_deferral_peer_bytes = bytes;
        }
        builder = builder.with_transport(defra_node::TransportConfig::Libp2p(
            defra_node::Libp2pConfig {
                listen_addrs: vec![listen.to_owned()],
                sync,
                ..Default::default()
            },
        ));
    }
    if let Some(config) = http {
        builder = builder.with_http(config);
    }
    if spec.node_identity.is_some() || spec.signing.is_some() {
        builder = builder.with_signing_key(signing_key(spec));
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

/// The key a node signs with: its node identity's, or, for a node that only
/// signs, a key of its own that no test identity shares.
fn signing_key(spec: &NodeSpec) -> defra_node::SigningKey {
    let seed = spec.node_identity.unwrap_or(u64::MAX);
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
    if spec.node_identity.is_none() && spec.signing.is_none() {
        return None;
    }
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
    /// Gossip heads the node dropped past a deferral budget, not yet awaited.
    gossip_dropped: VecDeque<events::P2PGossipHeadDroppedData>,
    /// Every document a searchable-encryption artifact arrived for.
    se_synced: RapidHashSet<String>,
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
                self.update(|log| {
                    log.heads.insert(head.0.clone(), head.1.clone());
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

/// The peer ID at the end of a `/p2p/<ID>` address.
pub fn peer_id(address: &str) -> &str {
    address.rsplit("/p2p/").next().unwrap_or(address)
}
