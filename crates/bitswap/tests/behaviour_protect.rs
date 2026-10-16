use std::task::{Context, Poll};

use bitswap::handler::BitswapHandlerIn;
use bitswap::network::Network;
use bitswap::{Bitswap, Config};
use futures::task::noop_waker_ref;
use libp2p::core::transport::PortUse;
use libp2p::core::{ConnectedPoint, Endpoint};
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{ConnectionId, FromSwarm, NetworkBehaviour, NotifyHandler, ToSwarm};
use libp2p::PeerId;

mod common;
use common::mem_store::MemStore;

type Behaviour = Bitswap<MemStore>;

fn behaviour() -> Behaviour {
    let config = Config {
        server: None,
        ..Default::default()
    };
    Bitswap::new(PeerId::random(), MemStore::new(&[]), config)
}

fn establish(behaviour: &mut Behaviour, peer: PeerId, id: usize, others: usize) {
    let endpoint = ConnectedPoint::Dialer {
        address: "/ip4/127.0.0.1/tcp/1".parse().unwrap(),
        role_override: Endpoint::Dialer,
        port_use: PortUse::Reuse,
    };
    behaviour.on_swarm_event(FromSwarm::ConnectionEstablished(ConnectionEstablished {
        peer_id: peer,
        connection_id: ConnectionId::new_unchecked(id),
        endpoint: &endpoint,
        failed_addresses: &[],
        other_established: others,
    }));
}

fn responsive(behaviour: &mut Behaviour, peer: PeerId) {
    behaviour.on_identify(&peer, &["/ipfs/bitswap/1.2.0".to_string()]);
}

/// Handler events the behaviour queued, as (connection id, is protect).
fn drain(behaviour: &mut Behaviour) -> Vec<(usize, bool)> {
    let mut cx = Context::from_waker(noop_waker_ref());
    let mut out = Vec::new();
    while let Poll::Ready(event) = behaviour.poll(&mut cx) {
        let ToSwarm::NotifyHandler {
            handler: NotifyHandler::One(id),
            event,
            ..
        } = event
        else {
            panic!("unexpected swarm action");
        };
        out.push((
            format!("{id}").parse().unwrap(),
            matches!(event, BitswapHandlerIn::Protect),
        ));
    }
    out.sort_unstable();
    out
}

async fn protect(network: &Network, peer: PeerId) {
    network.protect_peer(peer).await;
}

/// Unprotects the peer, returning the handler events queued and the answer the caller got.
async fn unprotect(behaviour: &mut Behaviour, peer: PeerId) -> (Vec<(usize, bool)>, bool) {
    let network = behaviour.network().clone();
    let pending = tokio::spawn(async move { network.unprotect_peer(peer).await });
    tokio::task::yield_now().await;
    let events = drain(behaviour);
    (events, pending.await.unwrap())
}

#[tokio::test]
async fn protect_and_unprotect_reach_every_connection() {
    let mut b = behaviour();
    let peer = PeerId::random();
    establish(&mut b, peer, 1, 0);
    establish(&mut b, peer, 2, 1);
    responsive(&mut b, peer);

    protect(&b.network().clone(), peer).await;
    assert_eq!(drain(&mut b), [(1, true), (2, true)]);

    let (events, was_protected) = unprotect(&mut b, peer).await;
    assert_eq!(events, [(1, false), (2, false)]);
    assert!(was_protected);
}

#[tokio::test]
async fn a_connection_made_while_protected_starts_protected() {
    let mut b = behaviour();
    let peer = PeerId::random();
    establish(&mut b, peer, 1, 0);
    responsive(&mut b, peer);
    protect(&b.network().clone(), peer).await;
    assert_eq!(drain(&mut b), [(1, true)]);

    establish(&mut b, peer, 2, 1);
    assert_eq!(drain(&mut b), [(2, true)]);

    assert!(unprotect(&mut b, peer).await.1);
    establish(&mut b, peer, 3, 2);
    assert!(drain(&mut b).is_empty(), "no longer protected");
}

#[tokio::test]
async fn protect_is_ignored_for_a_peer_not_known_to_speak_bitswap() {
    let mut b = behaviour();
    let peer = PeerId::random();
    establish(&mut b, peer, 1, 0);

    protect(&b.network().clone(), peer).await;
    assert!(drain(&mut b).is_empty());
}
