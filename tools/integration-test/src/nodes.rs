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

use crate::client_type::{go_binary, go_shinzo_binary, network_edge, ClientType};
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
    /// A Go node built from the Shinzo pin, whatever the test's client: a
    /// mixed Rust and Go Shinzo cluster.
    pub go_shinzo: bool,
    /// A Rust node that speaks Go Shinzo's wire, under every client.
    pub shinzo_wire: bool,
    /// A Rust node under every client, its wire off.
    pub rust: bool,
    /// The node's retention floor, if any.
    pub retention: Option<crate::actions::RetentionSpec>,
    /// The node's replication filter cutoff, if any.
    pub refuse_at_or_below: Option<crate::actions::CutoffSpec>,
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
        let listen = match spec.listen.first() {
            Some(address) => address.clone(),
            None => "/ip4/127.0.0.1/tcp/0".to_owned(),
        };
        // A node that speaks Go Shinzo's wire, is pinned to Rust, or has a
        // retention floor or a receive filter (Rust options) is a Rust node
        // under every client; the Go clients reach it over HTTP, as rust-http
        // does.
        let rust = spec.shinzo_wire
            || spec.rust
            || spec.retention.is_some()
            || spec.refuse_at_or_below.is_some();
        let client_type = match client_type {
            _ if spec.go_shinzo => ClientType::GoShinzo,
            ClientType::GoHttp | ClientType::GoShinzo if rust => ClientType::RustHttp,
            other => other,
        };
        match client_type {
            ClientType::InProcess => {
                let node = Arc::new(build(&spec, &listen, None));
                Self::new(Some(node.clone()), None, spec, Some(node), Vec::new())
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
                let config = HttpConfig::with_listener(listener)
                    .expect("http config")
                    .with_dev_mode(true);
                let node = Arc::new(build(&spec, &listen, Some(config)));
                Self::new(None, Some(url), spec, Some(node), Vec::new())
            }
            ClientType::GoHttp => {
                let binary = go_binary().unwrap_or_else(|error| panic!("{error}"));
                Self::start_go(&binary, spec, listen, false)
            }
            ClientType::GoShinzo => {
                let binary = go_shinzo_binary().unwrap_or_else(|error| panic!("{error}"));
                Self::start_go(&binary, spec, listen, true)
            }
        }
    }

    fn start_go(binary: &std::path::Path, spec: NodeSpec, listen: String, on_disk: bool) -> Self {
        let options = GoOptions {
            p2p: spec.network.then_some(listen),
            dir: spec.dir.as_ref().map(|dir| dir.path().to_owned()),
            on_disk,
        };
        let node = GoNode::start(binary, &options).unwrap_or_else(|error| panic!("{error}"));
        let url = node.api_url().to_owned();
        Self::new(None, Some(url), spec, None, vec![Arc::new(node)])
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
            EventName::P2PNoPeers,
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

    /// The node's P2P addresses, as Go's `PeerInfo` lists them: the test's
    /// own view of the node, whichever client the test runs through.
    pub fn peer_info(&self) -> Result<Vec<String>, String> {
        let context = RequestContext::ambient();
        match (&self.embedded, &self.url) {
            (Some(node), _) => workers::block_on(node.peer_info_as(&context))
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string()),
            (None, Some(url)) => {
                let client = HttpClient::new(url).map_err(|e| e.to_string())?;
                workers::block_on(network_edge().spawn(async move { client.peer_info().await }))
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())
            }
            (None, None) => Ok(Vec::new()),
        }
    }
}

fn build(spec: &NodeSpec, listen: &str, http: Option<HttpConfig>) -> Node {
    // Every Rust node runs in this process. Go's harness encrypts with its
    // fixed test nonce, so expected `_commits` ciphertexts only match with it,
    // the in-process form of DEFRA_DETERMINISTIC_TEST_NONCE=1.
    crypto::encryption::nonce::set_deterministic_nonce(true);
    let mut builder = Node::builder();
    if let Some(dir) = &spec.dir {
        builder = builder.data_path(dir.path());
    }
    if spec.network {
        builder = builder.with_libp2p(listen);
    }
    if spec.shinzo_wire {
        builder = builder.with_shinzo_wire();
    }
    if let Some(retention) = &spec.retention {
        let mut floors = rapidhash::RapidHashMap::default();
        floors.insert(
            retention.collection_id.clone(),
            db::retention::RetentionFloor {
                field: retention.field.clone(),
                floor: retention.floor,
            },
        );
        builder = builder.with_retention(db::retention::StaticRetentionRule::new(floors));
    }
    if let Some(cutoff) = &spec.refuse_at_or_below {
        builder = builder.with_replication_filter(Arc::new(Cutoff(cutoff.clone())));
    }
    if let Some(config) = http {
        builder = builder.with_http(config);
    }
    workers::block_on(builder.build())
        .expect("run builder")
        .expect("start node")
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
    /// Pushes that found no peers, not yet awaited.
    no_peers: VecDeque<events::P2PNoPeersData>,
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
            MessageData::P2PNoPeers(event) => self.update(|log| log.no_peers.push_back(event)),
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

    /// The next push that found no peers.
    pub async fn no_peers(&self, deadline: Duration) -> Result<events::P2PNoPeersData, String> {
        // One deadline for the whole wait: other events arriving meanwhile do
        // not extend it.
        let end = std::time::Instant::now() + deadline;
        loop {
            let mut log = self.log.load_clone();
            if let Some(event) = log.no_peers.pop_front() {
                self.log.store(log);
                return Ok(event);
            }
            let left = end.saturating_duration_since(std::time::Instant::now());
            self.receive(left)
                .await
                .map_err(|reason| format!("no push found no peers: {reason}"))?;
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

/// The peer ID at the end of a `/p2p/<ID>` address.
pub fn peer_id(address: &str) -> &str {
    address.rsplit("/p2p/").next().unwrap_or(address)
}

/// A replication filter refusing documents whose integer field is at or
/// below a cutoff; a document without the field is kept.
struct Cutoff(crate::actions::CutoffSpec);

impl db::replication_filter::ReplicationFilter for Cutoff {
    fn allow_replication(
        &self,
        _collection_id: &str,
        _doc_id: &str,
        fields: &db::replication_filter::FieldValues,
    ) -> bool {
        match fields.get(&self.0.field) {
            Some(ciborium::Value::Integer(value)) => i128::from(*value) > i128::from(self.0.cutoff),
            _ => true,
        }
    }
}
