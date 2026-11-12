//! The ownership guards of `proofs/tla/GossipOwnership.tla`, checked on the
//! driver: gossip reaches a peer only over a live connection or a dial the
//! endpoint admits, and forgetting a peer never closes its connection.
//! Also the driver's own bookkeeping: a dial that lands on an older mux ends
//! the gossip dial, and peers' gossiped addresses never fill up for good.

use iroh::endpoint::Connection;
use iroh_gossip::proto::{Command, InEvent};
use rapidhash::fast::RandomState;
use regolith::sync::{HopscotchMap, SegQueue};

use super::super::frame::{MAX_BROADCAST, TOPIC_ID_LEN};
use super::super::GossipHandle;
use super::*;
use crate::iroh::endpoint::shutdown_tracked_tasks;
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
        Arc::new(TaskRegistry::new(edge::testing::edge())),
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
    let accept = nuclei::spawn(edge::compat({
        let server = server.clone();
        async move { server.accept().await.unwrap().await.unwrap() }
    }));
    let connection = client.connect(server.addr(), alpn).await.expect("connect");
    let far_side = accept.join().await.unwrap();
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

#[nuclei::test(threads = 0)]
async fn a_peer_with_no_connection_is_reached_only_through_an_admitted_dial() {
    edge::compat(async {
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
    })
    .await
}

#[nuclei::test(threads = 0)]
async fn forgetting_a_peer_leaves_its_connection_open() {
    edge::compat(async {
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
        let done = nuclei::time::timeout(Duration::from_secs(10), driver.io.inbox.next())
            .await
            .expect("the writer reports it is done");
        assert!(matches!(done, Some(Input::WriterDone { peer: p, .. }) if p == peer));
        assert!(connection.close_reason().is_none());
        shutdown_tracked_tasks(Arc::clone(&driver.io.tasks)).await;
    })
    .await
}

#[nuclei::test(threads = 0)]
async fn a_closed_connection_is_never_gossiped_on() {
    edge::compat(async {
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
    })
    .await
}

#[nuclei::test(threads = 0)]
async fn a_dial_that_lands_on_an_older_mux_ends_the_gossip_dial() {
    edge::compat(async {
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
        let input = nuclei::time::timeout(Duration::from_secs(10), driver.io.inbox.next())
            .await
            .expect("the gossip dial is answered")
            .expect("an input");
        assert!(matches!(input, Input::DialFailed(p) if p == peer));
        assert!(driver.on_input(input).await.is_ok());
        assert!(!driver.peers.contains_key(&peer));
    })
    .await
}

#[nuclei::test(threads = 0)]
async fn made_up_peers_never_keep_a_real_peers_address_out() {
    edge::compat(async {
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
    })
    .await
}

#[nuclei::test(threads = 0)]
async fn a_join_hint_dials_the_advertisers_of_a_joined_topic_only() {
    edge::compat(async {
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
    })
    .await
}

type ProtoState = proto::State<EndpointId, rand::rngs::StdRng>;

fn proto_state(me: EndpointId) -> ProtoState {
    proto::State::new(
        me,
        PeerData::new(Vec::new()),
        config(),
        rand::rngs::StdRng::seed_from_u64(1),
    )
}

/// Feeds `event` to `a` and runs `a` and `b` against each other until
/// neither has more to say. Returns what `a` sent `b`.
fn exchange(
    a: (EndpointId, &mut ProtoState),
    b: (EndpointId, &mut ProtoState),
    event: InEvent<EndpointId>,
) -> Vec<ProtoMessage> {
    let ((a_id, a), (b_id, b)) = (a, b);
    let mut sent = Vec::new();
    let mut queue = VecDeque::from([(true, event)]);
    while let Some((to_a, event)) = queue.pop_front() {
        let state = if to_a { &mut *a } else { &mut *b };
        let out: Vec<_> = state.handle(event, Instant::now(), None).collect();
        for event in out {
            if let OutEvent::SendMessage(_, message) = event {
                if to_a {
                    sent.push(message.clone());
                    queue.push_back((false, InEvent::RecvMessage(a_id, message)));
                } else {
                    queue.push_back((true, InEvent::RecvMessage(b_id, message)));
                }
            }
        }
    }
    sent
}

fn broadcast(content: Vec<u8>) -> InEvent<EndpointId> {
    InEvent::Command(
        topic_id("t"),
        Command::Broadcast(Bytes::from(content), Scope::Swarm),
    )
}

/// A broadcast the protocol accepts must reach the peer, and must not cost
/// the peer the messages published after it.
#[nuclei::test(threads = 0)]
async fn a_broadcast_the_protocol_sends_does_not_end_gossip_to_the_peer() {
    edge::compat(async {
        let dials = Dials::default();
        let (mut driver, _events) = driver(&dials);
        let (connection, far_side, _endpoints) = connection().await;
        let (a_id, b_id) = (iroh::SecretKey::generate().public(), connection.remote_id());
        let (mut a, mut b) = (proto_state(a_id), proto_state(b_id));
        let host = InEvent::Command(topic_id("t"), Command::Join(Vec::new()));
        b.handle(host, Instant::now(), None).for_each(drop);
        let join = InEvent::Command(topic_id("t"), Command::Join(vec![b_id]));
        exchange((a_id, &mut a), (b_id, &mut b), join);

        let limit = driver.io.limit;
        let size = |m: &ProtoMessage| postcard::to_stdvec(m).unwrap().len();
        assert!(
            exchange(
                (a_id, &mut a),
                (b_id, &mut b),
                broadcast(vec![1; 2 * limit])
            )
            .is_empty(),
            "the protocol drops a broadcast far over the limit itself"
        );
        let largest = (0..limit)
            .rev()
            .find_map(|len| exchange((a_id, &mut a), (b_id, &mut b), broadcast(vec![2; len])).pop())
            .expect("the protocol sends a broadcast just under the limit");
        let normal = exchange(
            (a_id, &mut a),
            (b_id, &mut b),
            broadcast(b"normal".to_vec()),
        )
        .pop()
        .expect("a normal broadcast is sent");
        eprintln!(
            "largest broadcast the protocol sends: {} byte frame; limit {limit}",
            size(&largest)
        );

        driver.connected(b_id, connection.clone());
        driver.send(b_id, largest);
        driver.send(b_id, normal.clone());

        let mut recv = nuclei::time::timeout(Duration::from_secs(10), far_side.accept_uni())
            .await
            .expect("a gossip stream opens")
            .expect("accept uni");
        protocols::read_stream_tag(&mut recv).await.unwrap();
        let mut arrived = Vec::new();
        loop {
            match nuclei::time::timeout(Duration::from_secs(5), read_gossip_frame(&mut recv, limit))
                .await
            {
                Ok(Ok(Some(message))) => arrived.push(size(&message)),
                other => {
                    eprintln!("stream ended: {other:?}");
                    break;
                }
            }
        }
        eprintln!("frames that arrived: {arrived:?}");
        assert!(
            arrived.contains(&size(&normal)),
            "the normal message published after the large one never arrived"
        );
        shutdown_tracked_tasks(Arc::clone(&driver.io.tasks)).await;
    })
    .await
}

/// Two protocol states that are neighbours on topic `t`.
fn neighbour_states() -> ((EndpointId, ProtoState), (EndpointId, ProtoState)) {
    let (a_id, b_id) = (
        iroh::SecretKey::generate().public(),
        iroh::SecretKey::generate().public(),
    );
    let (mut a, mut b) = (proto_state(a_id), proto_state(b_id));
    let host = InEvent::Command(topic_id("t"), Command::Join(Vec::new()));
    b.handle(host, Instant::now(), None).for_each(drop);
    let join = InEvent::Command(topic_id("t"), Command::Join(vec![b_id]));
    exchange((a_id, &mut a), (b_id, &mut b), join);
    ((a_id, a), (b_id, b))
}

/// `MAX_BROADCAST` is the protocol's own limit: it sends a broadcast that
/// size, as a frame within `FRAME_LIMIT`, and drops one a byte larger. This
/// pins `BROADCAST_OVERHEAD` to the iroh-gossip in use.
#[test]
fn the_broadcast_budget_is_the_protocols_own() {
    let ((a_id, mut a), (b_id, mut b)) = neighbour_states();
    let mut send = |len: usize, fill: u8| {
        exchange((a_id, &mut a), (b_id, &mut b), broadcast(vec![fill; len])).pop()
    };
    let largest = send(MAX_BROADCAST, 1).expect("the protocol sends a broadcast at the budget");
    let frame = postcard::to_stdvec(&largest).unwrap().len();
    assert!(
        frame <= FRAME_LIMIT,
        "{frame} byte frame over {FRAME_LIMIT}"
    );
    assert!(
        send(MAX_BROADCAST + 1, 2).is_none(),
        "the protocol drops a broadcast a byte over the budget"
    );
    for len in [0, 127, 128, 16_383, 16_384] {
        let fits = len <= MAX_BROADCAST;
        assert_eq!(send(len, 3 + (len % 200) as u8).is_some(), fits, "{len}");
    }
}

/// The frame limit allows for the topic our frame adds.
#[test]
fn a_frame_carries_the_topic_iroh_gossip_leaves_out() {
    assert_eq!(
        postcard::to_stdvec(&topic_id("t")).unwrap().len(),
        TOPIC_ID_LEN
    );
    assert_eq!(FRAME_LIMIT, config().max_message_size + TOPIC_ID_LEN);
}

/// A publish the protocol would drop is refused before it is queued.
#[test]
fn a_publish_the_protocol_would_drop_is_refused_before_it_is_queued() {
    let handle = GossipHandle::new();
    let too_large = handle.publish("t".into(), Bytes::from(vec![0; MAX_BROADCAST + 1]), false);
    assert!(
        matches!(too_large, Err(Error::GossipTooLarge { size, limit }) if size == MAX_BROADCAST + 1 && limit == MAX_BROADCAST),
        "{too_large:?}"
    );
    assert!(handle
        .publish("t".into(), Bytes::from(vec![0; MAX_BROADCAST]), false)
        .is_ok());
}

/// A message too large to frame is skipped, and the stream carries on with
/// the messages queued behind it.
#[nuclei::test(threads = 0)]
async fn a_message_too_large_to_frame_does_not_end_the_stream() {
    edge::compat(async {
        let dials = Dials::default();
        let (mut driver, _events) = driver(&dials);
        let (connection, far_side, _endpoints) = connection().await;
        let peer = connection.remote_id();
        let ((a_id, mut a), (b_id, mut b)) = neighbour_states();
        let large = exchange((a_id, &mut a), (b_id, &mut b), broadcast(vec![1; 1000]))
            .pop()
            .expect("sent");
        let normal = exchange(
            (a_id, &mut a),
            (b_id, &mut b),
            broadcast(b"normal".to_vec()),
        )
        .pop()
        .expect("sent");
        let size = |m: &ProtoMessage| postcard::to_stdvec(m).unwrap().len();
        driver.io.limit = size(&normal);

        driver.connected(peer, connection.clone());
        driver.send(peer, large);
        driver.send(peer, normal.clone());

        let mut recv = nuclei::time::timeout(Duration::from_secs(10), far_side.accept_uni())
            .await
            .expect("a gossip stream opens")
            .expect("accept uni");
        protocols::read_stream_tag(&mut recv).await.unwrap();
        let first = nuclei::time::timeout(
            Duration::from_secs(5),
            read_gossip_frame(&mut recv, FRAME_LIMIT),
        )
        .await
        .expect("a frame arrives")
        .unwrap()
        .expect("the stream is still open");
        assert_eq!(size(&first), size(&normal));
        shutdown_tracked_tasks(Arc::clone(&driver.io.tasks)).await;
    })
    .await
}
