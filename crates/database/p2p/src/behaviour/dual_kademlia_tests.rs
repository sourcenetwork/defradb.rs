use libp2p::swarm::{behaviour::DialFailure, DialError};

use super::*;

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
