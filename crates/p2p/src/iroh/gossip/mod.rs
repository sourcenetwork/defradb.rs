//! Gossip over the mux connections: iroh-gossip's protocol state, run by one
//! task that sends on the connections the endpoint already admitted.

mod delivery;
mod driver;
mod frame;
mod mailbox;

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use iroh::endpoint::Connection;
use iroh::{EndpointAddr, EndpointId, RelayUrl};
use iroh_gossip::proto::TopicId;
use rapidhash::fast::RandomState;
use regolith::sync::HopscotchMap;
use serde::{Deserialize, Serialize};

use crate::transport::{PeerAddr, PeerId};

use super::endpoint::{spawn_task, RawTopics, SpawnedTasks};
use super::peer_map::endpoint_id_to_peer_id;
use super::protocols;

use delivery::Delivery;
pub(super) use delivery::EventSender;
use driver::Driver;
use frame::ProtoMessage;
use mailbox::{Mailbox, Wake};

/// Inputs the driver may hold before senders wait.
const INBOX_CAPACITY: usize = 1024;

type Neighbors = Arc<HopscotchMap<EndpointId, (), RandomState>>;

/// The topics this node subscribed to, with their current neighbours.
type Subscribed = Arc<HopscotchMap<String, Neighbors, RandomState>>;

/// What the driver needs from the endpoint.
pub(super) trait Network {
    /// Dials `peer` through the endpoint's admission checks. The outcome
    /// comes back as [`GossipHandle::connected`] or
    /// [`GossipHandle::dial_failed`].
    fn dial(&self, peer: EndpointId, addrs: Vec<PeerAddr>);

    /// Our current address, for peers that learn about us through gossip.
    fn local_addr(&self) -> AddrInfo;
}

/// The address gossip passes around for a peer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct AddrInfo {
    relay_url: Option<RelayUrl>,
    direct_addresses: BTreeSet<SocketAddr>,
}

impl From<EndpointAddr> for AddrInfo {
    fn from(addr: EndpointAddr) -> Self {
        Self {
            relay_url: addr.relay_urls().next().cloned(),
            direct_addresses: addr.ip_addrs().copied().collect(),
        }
    }
}

impl AddrInfo {
    fn dial_hints(&self) -> Vec<PeerAddr> {
        self.direct_addresses
            .iter()
            .map(|addr| PeerAddr::new(addr.to_string()))
            .chain(
                self.relay_url
                    .iter()
                    .map(|url| PeerAddr::new(url.to_string())),
            )
            .collect()
    }
}

enum Input {
    Join(String),
    Quit(String),
    Publish {
        topic: String,
        payload: Bytes,
    },
    Connected {
        peer: EndpointId,
        connection: Connection,
    },
    Closed {
        peer: EndpointId,
        connection: usize,
    },
    WriterDone {
        peer: EndpointId,
        writer: u64,
    },
    DialFailed(EndpointId),
    Received {
        peer: EndpointId,
        message: ProtoMessage,
    },
}

/// The endpoint's handle to the gossip driver.
#[derive(Clone)]
pub(super) struct GossipHandle {
    inbox: Arc<Mailbox<Input>>,
    subscribed: Subscribed,
}

impl GossipHandle {
    /// A handle whose driver has not been started.
    pub(super) fn new() -> Self {
        Self {
            inbox: Arc::new(Mailbox::new(INBOX_CAPACITY, Wake::default())),
            subscribed: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
        }
    }

    /// Starts the driver for this handle.
    pub(super) fn start<N>(
        &self,
        me: EndpointId,
        network: N,
        event_tx: EventSender,
        raw_topics: RawTopics,
        tasks: SpawnedTasks,
    ) where
        N: Network + defra_core::thread_bounds::MaybeSend + 'static,
    {
        let delivery = Delivery {
            event_tx,
            raw_topics,
        };
        let driver = Driver::new(
            me,
            network,
            delivery,
            Arc::clone(&self.inbox),
            Arc::clone(&self.subscribed),
            Arc::clone(&tasks),
        );
        let _ = spawn_task(&tasks, driver.run());
    }

    /// Returns `false` when already subscribed.
    pub(super) async fn subscribe(&self, topic: String) -> bool {
        let neighbors = Arc::new(HopscotchMap::with_hasher(RandomState::default()));
        if self
            .subscribed
            .insert_if_absent(topic.clone(), neighbors)
            .is_some()
        {
            return false;
        }
        self.inbox.push(Input::Join(topic)).await;
        true
    }

    /// Returns `false` when not subscribed.
    pub(super) async fn unsubscribe(&self, topic: String) -> bool {
        if self.subscribed.remove(&topic).is_none() {
            return false;
        }
        self.inbox.push(Input::Quit(topic)).await;
        true
    }

    /// Broadcasts on `topic`. On a topic we are not subscribed to, the
    /// driver joins it, waits briefly for a neighbour, then leaves again.
    pub(super) async fn publish(&self, topic: String, payload: Bytes) {
        self.inbox.push(Input::Publish { topic, payload }).await;
    }

    pub(super) fn topic_peers(&self, topic: &str) -> Vec<PeerId> {
        self.subscribed
            .get(topic)
            .map(|neighbors| {
                neighbors
                    .keys()
                    .map(|id| endpoint_id_to_peer_id(&id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// An admitted connection to `peer` is open. Only peers on the current
    /// mux ALPN gossip; older peers keep every other protocol.
    pub(super) async fn connected(&self, peer: EndpointId, connection: &Connection) {
        if connection.alpn() == protocols::ALPN_MUX {
            let connection = connection.clone();
            self.inbox.push(Input::Connected { peer, connection }).await;
        }
    }

    pub(super) async fn dial_failed(&self, peer: EndpointId) {
        self.inbox.push(Input::DialFailed(peer)).await;
    }

    pub(super) fn close(&self) {
        self.inbox.close();
    }
}

fn topic_id(topic: &str) -> TopicId {
    TopicId::from(*blake3::hash(topic.as_bytes()).as_bytes())
}
