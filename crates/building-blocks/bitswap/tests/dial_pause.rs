//! Only a refusal of bitswap's own dial pauses its dialing: another behaviour's dial is refused for
//! reasons of its own, as DefraKademlia refuses a dial to a peer this node hung up on.

use std::task::{Context, Poll};
use std::time::Duration;

use bitswap::network::NetworkError;
use bitswap::{Bitswap, Config};
use futures::task::noop_waker_ref;
use libp2p::swarm::behaviour::DialFailure;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, DialError, FromSwarm, NetworkBehaviour, ToSwarm,
};
use libp2p::PeerId;

mod common;
use common::mem_store::MemStore;

type Behaviour = Bitswap<MemStore>;

fn behaviour() -> Behaviour {
    let config = Config {
        server: None,
        ..Default::default()
    };
    Bitswap::new(
        PeerId::random(),
        MemStore::new(&[]),
        config,
        edge::testing::edge(),
    )
}

#[derive(Debug)]
struct Refused;

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("refused")
    }
}

impl std::error::Error for Refused {}

fn deny(behaviour: &mut Behaviour, peer: PeerId, connection_id: ConnectionId) {
    let error = DialError::Denied {
        cause: ConnectionDenied::new(Refused),
    };
    behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
        peer_id: Some(peer),
        error: &error,
        connection_id,
    }));
}

/// Asks the behaviour to dial `peer`: the connection id of the dial it hands the swarm, or the
/// error the caller got when it hands none.
async fn dial(behaviour: &mut Behaviour, peer: PeerId) -> Result<ConnectionId, NetworkError> {
    let network = behaviour.network().clone();
    let pending =
        nuclei::spawn_local(async move { network.dial(peer, Duration::from_secs(5)).await });
    nuclei::task::yield_now().await;
    let mut cx = Context::from_waker(noop_waker_ref());
    while let Poll::Ready(event) = behaviour.poll(&mut cx) {
        if let ToSwarm::Dial { opts } = event {
            return Ok(opts.connection_id());
        }
    }
    nuclei::task::yield_now().await;
    Err(pending.join().await.unwrap().unwrap_err())
}

#[nuclei::test]
async fn another_behaviours_refused_dial_leaves_dialing_on() {
    let mut b = behaviour();
    deny(&mut b, PeerId::random(), ConnectionId::new_unchecked(7));

    assert!(
        dial(&mut b, PeerId::random()).await.is_ok(),
        "a refused Kademlia dial must not pause bitswap's dials"
    );
}

#[nuclei::test]
async fn a_refused_dial_of_its_own_pauses_dialing() {
    let mut b = behaviour();
    let peer = PeerId::random();
    let ours = dial(&mut b, peer).await.expect("the first dial goes out");
    deny(&mut b, peer, ours);

    let paused = dial(&mut b, PeerId::random()).await.unwrap_err();
    assert!(paused.to_string().contains("dialing paused"), "{paused}");
}
