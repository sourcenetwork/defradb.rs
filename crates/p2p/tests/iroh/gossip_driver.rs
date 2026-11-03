//! The ownership guards of `proofs/tla/GossipOwnership.tla`, checked on the
//! driver: gossip reaches a peer only over a live connection or a dial the
//! endpoint admits, and forgetting a peer never closes its connection.
//! Also the driver's own bookkeeping: a dial that lands on an older mux ends
//! the gossip dial, and peers' gossiped addresses never fill up for good.

use iroh::endpoint::Connection;
use iroh_gossip::proto::{Command, InEvent};
use rapidhash::fast::RandomState;
use regolith::sync::{HopscotchMap, SegQueue};

use super::super::GossipHandle;
use super::*;
use crate::iroh::task_registry::TaskRegistry;
use crate::sync::DispatchReceiver;
use crate::transport::TransportEvent;

/// Records the dials the driver asks the endpoint for.
#[derive(Clone, Default)]
struct Dials(Arc<SegQueue<EndpointId>>);

impl Network for Dials {
    fn dial(&self, peer: EndpointId, _addrs: Vec<PeerAddr>) {
        self.0.push(peer);
    }

    fn local_addr(&self) -> AddrInfo {
        AddrInfo::default()
    }
}

type Events = DispatchReceiver<TransportEvent<iroh::endpoint::SendStream>>;

fn driver(dials: &Dials) -> (Driver<Dials>, Events) {
    let (event_tx, events) = crate::sync::dispatch_queue(16);
    let delivery = Delivery {
        event_tx,
        raw_topics: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
        records: crate::iroh::records::RecordsHandle::new(),
    };
    let driver = Driver::new(
        iroh::SecretKey::generate().public(),
        dials.clone(),
        delivery,
        Arc::new(Inbox::new(16)),
        Arc::new(HopscotchMap::with_hasher(RandomState::default())),
        Arc::new(TaskRegistry::default()),
    );
    (driver, events)
}

async fn endpoint() -> iroh::Endpoint {
    iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![
            protocols::ALPN_MUX.to_vec(),
            protocols::ALPN_MUX_V1.to_vec(),
        ])
        .bind_addr("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
        .expect("bind addr")
        .bind()
        .await
        .expect("bind endpoint")
}

/// A live mux connection, with both endpoints and the far side kept open.
async fn connection() -> (Connection, Connection, [iroh::Endpoint; 2]) {
    connection_on(protocols::ALPN_MUX).await
}

async fn connection_on(alpn: &[u8]) -> (Connection, Connection, [iroh::Endpoint; 2]) {
    let server = endpoint().await;
    let client = endpoint().await;
    let accept = n0_future::task::spawn({
        let server = server.clone();
        async move { server.accept().await.unwrap().await.unwrap() }
    });
    let connection = client.connect(server.addr(), alpn).await.expect("connect");
    let far_side = accept.await.unwrap();
    (connection, far_side, [server, client])
}

/// Makes the protocol send `peer` a message: joining a topic through it.
async fn gossip_to(driver: &mut Driver<Dials>, peer: EndpointId) {
    let topic = topic_id("t");
    driver
        .queued
        .push_back(InEvent::Command(topic, Command::Join(vec![peer])));
    assert!(driver.drain().await.is_ok());
}

#[tokio::test]
async fn a_peer_with_no_connection_is_reached_only_through_an_admitted_dial() {
    let dials = Dials::default();
    let (mut driver, _events) = driver(&dials);
    let peer = iroh::SecretKey::generate().public();

    gossip_to(&mut driver, peer).await;
    assert_eq!(dials.0.pop(), Some(peer));
    let entry = &driver.peers[&peer];
    assert!(entry.writer.is_none() && entry.dialing);
    assert_eq!(entry.pending.len(), 1);

    // The endpoint refused the dial, as it does for a revoked peer.
    assert!(driver.on_input(Input::DialFailed(peer)).await.is_ok());
    assert!(!driver.peers.contains_key(&peer));
}

#[tokio::test]
async fn forgetting_a_peer_leaves_its_connection_open() {
    let dials = Dials::default();
    let (mut driver, _events) = driver(&dials);
    let (connection, _far_side, _endpoints) = connection().await;
    let peer = connection.remote_id();

    driver.connected(peer, connection.clone());
    gossip_to(&mut driver, peer).await;
    assert!(driver.peers[&peer].writer.is_some());
    assert!(dials.0.pop().is_none());

    driver.forget(peer);
    assert!(driver.peers[&peer].writer.is_none());
    let done = tokio::time::timeout(Duration::from_secs(10), driver.io.inbox.next())
        .await
        .expect("the writer reports it is done");
    assert!(matches!(done, Some(Input::WriterDone { peer: p, .. }) if p == peer));
    assert!(connection.close_reason().is_none());
}

#[tokio::test]
async fn a_closed_connection_is_never_gossiped_on() {
    let dials = Dials::default();
    let (mut driver, _events) = driver(&dials);
    let (connection, _far_side, _endpoints) = connection().await;
    let peer = connection.remote_id();
    connection.close(0u32.into(), b"revoked");

    driver.connected(peer, connection);
    assert!(!driver.peers.contains_key(&peer));
    gossip_to(&mut driver, peer).await;
    assert!(driver.peers[&peer].writer.is_none());
    assert_eq!(dials.0.pop(), Some(peer));
}

#[tokio::test]
async fn a_dial_that_lands_on_an_older_mux_ends_the_gossip_dial() {
    let dials = Dials::default();
    let (mut driver, _events) = driver(&dials);
    let (connection, _far_side, _endpoints) = connection_on(protocols::ALPN_MUX_V1).await;
    let peer = connection.remote_id();
    gossip_to(&mut driver, peer).await;
    assert!(driver.peers[&peer].dialing);

    let handle = GossipHandle {
        inbox: Arc::clone(&driver.io.inbox),
        subscribed: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
    };
    handle.connected(peer, &connection);
    let input = tokio::time::timeout(Duration::from_secs(10), driver.io.inbox.next())
        .await
        .expect("the gossip dial is answered")
        .expect("an input");
    assert!(matches!(input, Input::DialFailed(p) if p == peer));
    assert!(driver.on_input(input).await.is_ok());
    assert!(!driver.peers.contains_key(&peer));
}

#[tokio::test]
async fn made_up_peers_never_keep_a_real_peers_address_out() {
    let dials = Dials::default();
    let (mut driver, _events) = driver(&dials);
    for _ in 0..MAX_LEARNED {
        driver
            .learned
            .insert(iroh::SecretKey::generate().public(), Vec::new());
    }
    let peer = iroh::SecretKey::generate().public();
    driver.learn(peer, &peer_data(&AddrInfo::default()));
    assert!(driver.learned.contains_key(&peer));
    assert_eq!(driver.learned.len(), MAX_LEARNED);
}

#[tokio::test]
async fn a_join_hint_dials_the_advertisers_of_a_joined_topic_only() {
    let dials = Dials::default();
    let (mut driver, _events) = driver(&dials);
    let peer = iroh::SecretKey::generate().public();
    let hint = || Input::Hint {
        topic: "t".into(),
        peers: vec![peer],
    };

    assert!(driver.on_input(hint()).await.is_ok());
    assert!(driver.drain().await.is_ok());
    assert!(
        dials.0.pop().is_none(),
        "a topic we have not joined is left alone"
    );

    assert!(driver.on_input(Input::Join("t".into())).await.is_ok());
    assert!(driver.on_input(hint()).await.is_ok());
    assert!(driver.drain().await.is_ok());
    assert_eq!(dials.0.pop(), Some(peer));
}
