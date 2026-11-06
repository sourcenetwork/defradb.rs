//! Embedder stream protocols (`defra.stream_protocols`): iroh nodes that serve
//! test protocols, and the actions that open them.
//!
//! Stream protocols are a Rust embedder API with no HTTP, CLI or Go surface,
//! so every action here runs under the in-process client only. Under any
//! other client they stop with `unsupported op`, which the testo report counts
//! as unsupported rather than failed.

mod admit;
mod new_iroh_node;
mod open;
mod read_kept;

pub use admit::{AllowStreamPeer, DenyStreamPeer};
pub use new_iroh_node::NewIrohNode;
pub use open::{OpenStream, ReadMode};
pub use read_kept::ReadKeptStream;

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use defra_node::{Node, NodeBuilder, P2PConfig};
use p2p::iroh::{
    InboundStream, Priority, ProtocolRecvStream, ProtocolSendStream, StreamHandler, StreamProtocol,
};
use rapidhash::fast::RandomState;
use regolith::sync::{HopscotchMap, OnceCell, SegQueue};
use serde::{Deserialize, Serialize};

use crate::scenario::Failure;
use crate::test_case::State;

/// What a stuck or holding handler writes once it is serving, so an opener
/// knows its stream reached a handler.
pub const STARTED: &[u8] = b"started";

/// How a test protocol's handler behaves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Handler {
    /// Replies `{"peer": <opener>, "request": <request>}` and finishes.
    #[default]
    Echo,
    /// Runs the request as a GraphQL query on its node, through the node's
    /// request path, and replies with the result's data.
    Query,
    /// Drops its request, writes `started` and never returns.
    Stuck,
    /// Keeps its request, writes `started` and never returns.
    Hold,
}

/// A request limit: bytes, or a peer's share of the priority's budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Size {
    Bytes(usize),
    /// One peer's whole share of the protocol's priority.
    Share,
    /// One byte more than that.
    AboveShare,
}

impl Default for Size {
    fn default() -> Self {
        Self::Bytes(1024)
    }
}

/// A test protocol, as a scenario names it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolSpec {
    pub tag: String,
    /// `control`, `replication`, `sync` (the default) or `bulk`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default)]
    pub max_message: Size,
    /// The handler's deadline; 30 s when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_millis: Option<u64>,
    #[serde(default)]
    pub handler: Handler,
}

impl ProtocolSpec {
    fn priority(&self) -> Result<Priority, Failure> {
        match self.priority.as_deref().unwrap_or("sync") {
            "control" => Ok(Priority::Control),
            "replication" => Ok(Priority::Replication),
            "sync" => Ok(Priority::Sync),
            "bulk" => Ok(Priority::Bulk),
            other => Err(Failure::operation(format!("no priority {other}"))),
        }
    }

    /// The protocol this spec names.
    pub fn protocol(&self) -> Result<StreamProtocol, Failure> {
        let priority = self.priority()?;
        let share = p2p::iroh::peer_share_bytes(priority);
        Ok(StreamProtocol {
            // ponytail: a test protocol's tag lives for the test process.
            tag: Box::leak(self.tag.clone().into_bytes().into_boxed_slice()),
            priority,
            max_message: match self.max_message {
                Size::Bytes(bytes) => bytes,
                Size::Share => share,
                Size::AboveShare => share + 1,
            },
            deadline: Duration::from_millis(self.deadline_millis.unwrap_or(30_000)),
        })
    }
}

/// How an iroh node of a test starts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IrohSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<ProtocolSpec>,
    /// Admit only peers allowed with `AllowStreamPeer`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub explicit_allowlist: bool,
    /// Build the node with no P2P at all, still registering the protocols.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub without_network: bool,
}

/// Build an in-process node as `spec` says, its handlers bound to it.
pub fn build(spec: &IrohSpec) -> Result<Arc<Node>, String> {
    let slot: Arc<OnceCell<Weak<Node>>> = Arc::new(OnceCell::new());
    let mut builder = NodeBuilder::default();
    if !spec.without_network {
        builder = builder.with_p2p(P2PConfig {
            bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            port: Some(0),
            relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
            discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
            allowlist: match spec.explicit_allowlist {
                true => p2p::iroh::IrohAllowlistConfig::Explicit(Default::default()),
                false => p2p::iroh::IrohAllowlistConfig::AcceptAll,
            },
            ..Default::default()
        });
    }
    for protocol in &spec.protocols {
        let handler = Arc::new(TestHandler {
            behaviour: protocol.handler,
            node: Arc::clone(&slot),
        });
        let built = protocol.protocol().map_err(|failure| failure.message)?;
        builder = builder.with_stream_protocol(built, handler);
    }
    let node = Arc::new(
        workers::block_on(builder.build())
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?,
    );
    let _ = slot.set(Arc::downgrade(&node));
    Ok(node)
}

struct TestHandler {
    behaviour: Handler,
    node: Arc<OnceCell<Weak<Node>>>,
}

#[async_trait]
impl StreamHandler for TestHandler {
    async fn handle(&self, stream: InboundStream) {
        let InboundStream {
            peer,
            request,
            mut send,
            recv,
            ..
        } = stream;
        match self.behaviour {
            Handler::Echo => {
                let reply = serde_json::json!({
                    "peer": peer.to_string(),
                    "request": String::from_utf8_lossy(&request),
                });
                drop(request);
                if send.write_all(reply.to_string().as_bytes()).await.is_ok() {
                    let _ = send.finish();
                }
            }
            Handler::Query => {
                // Dropping `send` unfinished resets the stream: the node is gone.
                let Some(node) = self.node.get().and_then(Weak::upgrade) else {
                    return;
                };
                let query = String::from_utf8_lossy(&request).into_owned();
                drop(request);
                let response = node.execute(&query).await;
                let reply = serde_json::to_vec(&response.data).unwrap_or_default();
                if send.write_all(&reply).await.is_ok() {
                    let _ = send.finish();
                }
            }
            Handler::Stuck => {
                drop(request);
                let _ = send.write_all(STARTED).await;
                let _halves = (send, recv);
                std::future::pending::<()>().await;
            }
            Handler::Hold => {
                let _ = send.write_all(STARTED).await;
                let _held = (request, send, recv);
                std::future::pending::<()>().await;
            }
        }
    }
}

/// A stream an opener keeps open across actions.
pub struct KeptStream {
    pub send: ProtocolSendStream,
    pub recv: ProtocolRecvStream,
}

/// The streams a test keeps open, by name. Each name holds a one-slot queue,
/// so a stream is taken out by value: the map's removed entries may outlive
/// the removal, an owned stream may not.
#[derive(Clone)]
pub struct KeptStreams(Arc<HopscotchMap<String, Arc<SegQueue<KeptStream>>, RandomState>>);

impl Default for KeptStreams {
    fn default() -> Self {
        Self(Arc::new(HopscotchMap::with_hasher(RandomState::default())))
    }
}

impl KeptStreams {
    pub fn keep(&self, name: &str, stream: KeptStream) {
        let slot = Arc::new(SegQueue::new());
        slot.push(stream);
        self.0.insert(name.to_owned(), slot);
    }

    /// Take stream `name` out, to read it to its end.
    pub fn take(&self, name: &str) -> Result<KeptStream, Failure> {
        self.0
            .get(name)
            .and_then(|slot| slot.pop())
            .ok_or_else(|| Failure::operation(format!("no kept stream {name}")))
    }
}

/// The in-process node `name` runs, or `unsupported` under another client.
pub fn in_process(state: &State, name: &str, op: &str) -> Result<Arc<Node>, Failure> {
    state.node(name)?.node.clone().ok_or_else(|| {
        Failure::unsupported(format!(
            "{op}: stream protocols are an embedder API with no HTTP, CLI or Go surface"
        ))
    })
}

/// Node `node`'s iroh peer ID.
pub fn peer_id(state: &State, node: &Node) -> Result<String, Failure> {
    let system = node
        .p2p_system()
        .ok_or_else(|| Failure::operation("the node has no P2P".into()))?;
    state
        .block_on(system.ops().local_peer_id())
        .map_err(|e| Failure::operation(e.to_string()))
}

/// An address node `node` can be dialled at, once iroh has one.
pub fn address(state: &State, node: &Node) -> Result<String, Failure> {
    let system = node
        .p2p_system()
        .ok_or_else(|| Failure::operation("the node has no P2P".into()))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let addresses = state
            .block_on(system.ops().listen_addresses())
            .map_err(|e| Failure::operation(e.to_string()))?;
        if let Some(address) = addresses
            .into_iter()
            .find(|a| a.contains("/p2p/") || a.starts_with("endpoint"))
        {
            return Ok(address);
        }
        if std::time::Instant::now() >= deadline {
            return Err(Failure::operation("no dialable iroh address".into()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
