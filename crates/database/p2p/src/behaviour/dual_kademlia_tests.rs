//! What Kademlia does on its own: how many bootstraps a small table runs at
//! once, and which peers its own inserts may put in the WAN table (#2992).

use std::time::Duration;

use futures::StreamExt;
use libp2p::kad;
use libp2p::swarm::{dial_opts::DialOpts, NetworkBehaviour, Swarm, SwarmEvent};
use libp2p::{identify, identity::Keypair, Multiaddr, PeerId};

use super::dual_kademlia::{DefraKademliaEvent, DualKademlia, KademliaNetwork};
use crate::host::transport::{build, Executor};

fn bootstraps(dual: &DualKademlia, network: KademliaNetwork) -> usize {
    let kademlia = match network {
        KademliaNetwork::Lan => &dual.lan,
        KademliaNetwork::Wan => &dual.wan,
    };
    kademlia
        .iter_queries()
        .filter(|query| matches!(query.info(), kad::QueryInfo::Bootstrap { .. }))
        .count()
}

/// Go runs one low-peer refresh at a time (`fixLowPeersChan` holds one
/// request, `RefreshNoWait` drops one while a refresh runs). Ten peers
/// joining at once start one bootstrap per table, not ten.
#[test]
fn ten_new_peers_start_one_bootstrap_per_table() {
    let mut dual = DualKademlia::new(PeerId::random());
    for i in 1..=10u8 {
        let address: Multiaddr = format!("/ip4/8.8.8.{i}/tcp/4001").parse().unwrap();
        dual.add_address(&PeerId::random(), address);
        dual.refresh_if_low(KademliaNetwork::Wan);
        dual.refresh_if_low(KademliaNetwork::Lan);
    }
    assert_eq!(bootstraps(&dual, KademliaNetwork::Wan), 1);
    assert_eq!(bootstraps(&dual, KademliaNetwork::Lan), 1);
}

#[derive(NetworkBehaviour)]
#[behaviour(prelude = "libp2p::swarm::derive_prelude")]
struct Node {
    kademlia: DualKademlia,
    identify: identify::Behaviour,
}

fn node() -> Swarm<Node> {
    let key = Keypair::generate_ed25519();
    let id = key.public().to_peer_id();
    let (transport, _) = build(&key, false).unwrap();
    Swarm::new(
        transport,
        Node {
            kademlia: DualKademlia::new(id),
            identify: identify::Behaviour::new(identify::Config::new(
                "/defra-test/1".into(),
                key.public(),
            )),
        },
        id,
        libp2p::swarm::Config::with_executor(Executor::new(edge::testing::edge()))
            .with_idle_connection_timeout(Duration::from_secs(60)),
    )
}

/// A refresh asked for while one runs is run once, when the first ends.
#[nuclei::test]
async fn a_refresh_asked_during_a_bootstrap_runs_once_after_it() {
    crate::rt::time::timeout(
        Duration::from_secs(30),
        edge::compat(async {
            let mut swarm = node();
            // Nothing listens on port 1: each dial fails at once, so each
            // bootstrap ends quickly.
            for _ in 0..3 {
                let address: Multiaddr = "/ip4/127.0.0.1/tcp/1".parse().unwrap();
                swarm
                    .behaviour_mut()
                    .kademlia
                    .lan
                    .add_address(&PeerId::random(), address);
            }
            let kademlia = &mut swarm.behaviour_mut().kademlia;
            assert!(kademlia.refresh_if_low(KademliaNetwork::Lan).is_some());
            assert!(kademlia.refresh_if_low(KademliaNetwork::Lan).is_none());
            assert!(kademlia.refresh_if_low(KademliaNetwork::Lan).is_none());

            let mut finished = 0;
            while finished < 2 {
                if let SwarmEvent::Behaviour(NodeEvent::Kademlia(DefraKademliaEvent::Lan(
                    kad::Event::OutboundQueryProgressed {
                        result: kad::QueryResult::Bootstrap(_),
                        step,
                        ..
                    },
                ))) = swarm.select_next_some().await
                {
                    if step.last {
                        finished += 1;
                        let running = bootstraps(&swarm.behaviour().kademlia, KademliaNetwork::Lan);
                        let expected = if finished == 1 { 1 } else { 0 };
                        assert_eq!(running, expected, "bootstraps after {finished} ended");
                    }
                }
            }
        }),
    )
    .await
    .expect("both bootstraps ended within the deadline");
}

/// Kademlia inserts a peer it dialed at the dialed address, without asking
/// `add_address`. A loopback peer serving the WAN protocol, as a Go node with
/// public reachability does, must still stay out of the WAN table.
#[nuclei::test]
async fn a_dialed_loopback_peer_stays_out_of_the_wan_table() {
    crate::rt::time::timeout(
        Duration::from_secs(30),
        edge::compat(async {
            let mut dialer = node();
            let mut server = node();
            server
                .behaviour_mut()
                .kademlia
                .wan
                .set_mode(Some(kad::Mode::Server));
            server
                .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
                .unwrap();
            let address = loop {
                if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                    break address;
                }
            };
            let server_id = *server.local_peer_id();
            nuclei::spawn(async move {
                loop {
                    server.select_next_some().await;
                }
            })
            .detach();

            dialer
                .dial(
                    DialOpts::peer_id(server_id)
                        .addresses(vec![address])
                        .build(),
                )
                .unwrap();
            // The refusal happens inside the swarm's poll and emits nothing,
            // so the loop also wakes on a short tick to read its count.
            let mut tick = crate::rt::time::interval(Duration::from_millis(20));
            while dialer.behaviour().kademlia.wan.private_refused() == 0 {
                crate::rt::select! {
                    event = dialer.select_next_some() => {
                        if let SwarmEvent::Behaviour(NodeEvent::Kademlia(DefraKademliaEvent::Wan(
                            kad::Event::RoutingUpdated { addresses, .. },
                        ))) = event
                        {
                            panic!("a loopback peer entered the WAN table at {addresses:?}");
                        }
                    }
                    _ = tick.tick() => {}
                }
            }
            let wan_peers: usize = dialer
                .behaviour_mut()
                .kademlia
                .wan
                .kbuckets()
                .map(|bucket| bucket.num_entries())
                .sum();
            assert_eq!(wan_peers, 0, "the WAN table holds no loopback peer");
        }),
    )
    .await
    .expect("the WAN table refused the loopback peer within the deadline");
}
