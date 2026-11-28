use libp2p::swarm::{behaviour::DialFailure, DialError};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::Transport;
use regolith::sync::SegQueue;
use tracing::{
    field::{Field, Visit},
    span, Event, Level, Metadata, Subscriber,
};

use super::*;

#[derive(Default)]
struct Message(Option<String>);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.0 = Some(format!("{value:?}"));
        }
    }
}

struct CaptureWarnings(Arc<SegQueue<String>>);

impl Subscriber for CaptureWarnings {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= &Level::WARN
    }

    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut message = Message::default();
        event.record(&mut message);
        if let Some(message) = message.0 {
            self.0.push(message);
        }
    }

    fn enter(&self, _: &span::Id) {}

    fn exit(&self, _: &span::Id) {}
}

fn held_kademlia(peer: PeerId) -> DefraKademlia {
    let mut kademlia = DefraKademlia::new(PeerId::random(), KademliaNetwork::Wan);
    kademlia.redial_hold.hold(peer, Instant::now());
    kademlia
}

fn pending_outbound(
    kademlia: &mut DefraKademlia,
    connection_id: ConnectionId,
    peer: PeerId,
) -> Result<Vec<Multiaddr>, ConnectionDenied> {
    kademlia.handle_pending_outbound_connection(connection_id, Some(peer), &[], Endpoint::Dialer)
}

#[test]
fn its_own_dial_to_a_held_peer_is_refused_once_it_reaches_the_swarm() {
    let peer = PeerId::random();
    let mut kademlia = held_kademlia(peer);
    let own = ConnectionId::new_unchecked(1);
    kademlia.own_dials.insert(own);

    assert!(pending_outbound(&mut kademlia, own, peer).is_err());
    assert!(kademlia.own_dials.is_empty());
}

#[test]
fn another_behaviours_dial_to_a_held_peer_is_permitted() {
    let peer = PeerId::random();
    let mut kademlia = held_kademlia(peer);

    assert!(pending_outbound(&mut kademlia, ConnectionId::new_unchecked(2), peer).is_ok());
}

#[test]
fn a_dial_that_failed_its_peer_condition_is_forgotten() {
    let peer = PeerId::random();
    let mut kademlia = held_kademlia(peer);
    let own = ConnectionId::new_unchecked(3);
    kademlia.own_dials.insert(own);

    let error = DialError::DialPeerConditionFalse(Default::default());
    kademlia.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(peer),
        error: &error,
        connection_id: own,
    }));

    assert!(kademlia.own_dials.is_empty());
}

fn closed_last_connection(kademlia: &mut DefraKademlia, peer: PeerId, remaining: usize) {
    let endpoint = libp2p::core::ConnectedPoint::Listener {
        local_addr: "/ip4/127.0.0.1/tcp/4001".parse().expect("local address"),
        send_back_addr: "/ip4/127.0.0.1/tcp/4002".parse().expect("remote address"),
    };
    kademlia.on_swarm_event(FromSwarm::ConnectionClosed(
        libp2p::swarm::behaviour::ConnectionClosed {
            peer_id: peer,
            connection_id: ConnectionId::new_unchecked(10),
            endpoint: &endpoint,
            cause: None,
            remaining_established: remaining,
        },
    ));
}

/// Every dial the inner Kademlia asks the swarm for until it has nothing
/// more to do: the connection id and the peer of each.
fn kademlia_dials(kademlia: &mut DefraKademlia) -> Vec<(ConnectionId, Option<PeerId>)> {
    let mut cx = Context::from_waker(std::task::Waker::noop());
    let mut dials = Vec::new();
    while let Poll::Ready(event) = kademlia.poll(&mut cx) {
        if let ToSwarm::Dial { opts } = event {
            dials.push((opts.connection_id(), opts.get_peer_id()));
        }
    }
    dials
}

/// A Kademlia that knows `peer` and has a query out that wants it.
fn kademlia_looking_for(peer: PeerId) -> DefraKademlia {
    let mut kademlia = DefraKademlia::new(PeerId::random(), KademliaNetwork::Wan);
    kademlia.add_address(
        &peer,
        "/ip4/127.0.0.1/tcp/4003".parse().expect("peer address"),
    );
    kademlia.get_closest_peers(PeerId::random());
    kademlia
}

#[test]
fn every_dial_kademlia_asks_for_is_remembered_as_its_own() {
    let peer = PeerId::random();
    let mut kademlia = kademlia_looking_for(peer);

    let dials = kademlia_dials(&mut kademlia);

    assert!(dials.iter().any(|(_, dialed)| *dialed == Some(peer)));
    for (connection_id, _) in &dials {
        assert!(kademlia.own_dials.contains(connection_id));
    }
}

#[test]
fn kademlias_dial_after_the_last_connection_closes_is_refused() {
    let peer = PeerId::random();
    let mut kademlia = kademlia_looking_for(peer);
    closed_last_connection(&mut kademlia, peer, 0);

    let (connection_id, _) = kademlia_dials(&mut kademlia)
        .into_iter()
        .find(|(_, dialed)| *dialed == Some(peer))
        .expect("Kademlia dials the peer its query wants");

    assert!(pending_outbound(&mut kademlia, connection_id, peer).is_err());
    assert_eq!(kademlia.held_refusals, 1);
}

#[test]
fn closing_one_of_several_connections_holds_nothing() {
    let peer = PeerId::random();
    let mut kademlia = kademlia_looking_for(peer);
    closed_last_connection(&mut kademlia, peer, 1);

    let (connection_id, _) = kademlia_dials(&mut kademlia)
        .into_iter()
        .find(|(_, dialed)| *dialed == Some(peer))
        .expect("Kademlia dials the peer its query wants");

    assert!(pending_outbound(&mut kademlia, connection_id, peer).is_ok());
}

/// A new listen address schedules a bootstrap warning after the Kademlia
/// throttle. With no peer to query, the wrapper must cancel it first.
#[nuclei::test]
async fn a_new_listen_address_with_no_peers_cancels_the_bootstrap_warning() {
    crate::rt::time::timeout(
        Duration::from_secs(10),
        edge::compat(async {
            let key = Keypair::generate_ed25519();
            let peer_id = key.public().to_peer_id();
            let transport = libp2p::core::transport::MemoryTransport::default()
                .upgrade(libp2p::core::upgrade::Version::V1)
                .authenticate(libp2p::noise::Config::new(&key).unwrap())
                .multiplex(libp2p::yamux::Config::default())
                .boxed();
            let mut swarm = libp2p::Swarm::new(
                transport,
                DefraKademlia::new(peer_id, KademliaNetwork::Wan),
                peer_id,
                libp2p::swarm::Config::with_executor(crate::host::transport::Executor::new(
                    edge::testing::edge(),
                )),
            );
            swarm.listen_on("/memory/1".parse().unwrap()).unwrap();

            let warnings = Arc::new(SegQueue::new());
            let dispatch = tracing::Dispatch::new(CaptureWarnings(Arc::clone(&warnings)));
            let mut listening = false;
            let _ = crate::rt::time::timeout(Duration::from_secs(2), async {
                loop {
                    let _ = std::future::poll_fn(|cx| {
                        tracing::dispatcher::with_default(&dispatch, || swarm.poll_next_unpin(cx))
                    })
                    .await;
                    listening |= swarm
                        .listeners()
                        .any(|address| address.to_string().starts_with("/memory/"));
                }
            })
            .await;

            assert!(listening, "the memory transport never started listening");
            while let Some(warning) = warnings.pop() {
                assert!(
                    !warning.contains("Failed to trigger bootstrap"),
                    "a peerless Kademlia warned after its listen bootstrap: {warning}"
                );
            }
        }),
    )
    .await
    .expect("the Kademlia warning test finished");
}
