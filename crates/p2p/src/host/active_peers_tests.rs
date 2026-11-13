//! `ActivePeers` lists a peer that dialed us at its listen address, even when
//! it reads before the peer's Identify arrives (#2774).
//!
//! The dialer here is a bare swarm that answers the host's Identify request
//! by hand, so the test holds the answer back while it asks `ActivePeers`.
//! Our dials come from a fresh port, which nothing listens on: without the
//! Identify, the only address the host has is that port.

use std::time::Duration;

use futures::{AsyncWriteExt, StreamExt};
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{Swarm, SwarmEvent};
use libp2p::{identity::Keypair, Multiaddr, StreamProtocol};

use crate::host::transport::{build, Executor};
use crate::testutil::MockBitswapStore;
use crate::P2PHost;

/// libp2p's Identify protocol; `/defra/identify/0.0.1` is only the version
/// string the host sends in it.
const IDENTIFY: &str = "/ipfs/id/1.0.0";

/// The address the dialer's Identify names as the one it listens on.
const LISTEN: &str = "/ip4/127.0.0.1/tcp/4242";

/// Append `value` as an unsigned varint.
fn varint(out: &mut Vec<u8>, mut value: usize) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// Append a length-delimited protobuf field.
fn field(out: &mut Vec<u8>, number: u8, bytes: &[u8]) {
    out.push((number << 3) | 2);
    varint(out, bytes.len());
    out.extend_from_slice(bytes);
}

/// The Identify message the dialer answers with, length-prefixed as the
/// protocol sends it: its public key, one listen address, an agent version.
fn identify_reply(key: &Keypair, listen: &Multiaddr) -> Vec<u8> {
    let mut message = Vec::new();
    field(&mut message, 1, &key.public().encode_protobuf());
    field(&mut message, 2, &listen.to_vec());
    field(&mut message, 5, b"/defra/identify/0.0.1");
    field(&mut message, 6, b"active-peers-test");
    let mut framed = Vec::new();
    varint(&mut framed, message.len());
    framed.extend_from_slice(&message);
    framed
}

#[nuclei::test]
async fn active_peers_lists_a_dialer_at_its_listen_address_before_its_identify() {
    crate::rt::time::timeout(
        Duration::from_secs(30),
        edge::compat(async {
            let (host, handle, _events, _replicators) =
                P2PHost::new(MockBitswapStore::new(), edge::testing::edge())
                    .await
                    .unwrap();
            nuclei::spawn(host.run()).detach();
            handle
                .listen("/ip4/127.0.0.1/tcp/0".parse().unwrap())
                .await
                .unwrap();
            let host_addr = handle.listen_addresses().await.unwrap().remove(0);
            let host_id = handle.local_peer_id_cached();

            let key = Keypair::generate_ed25519();
            let dialer_id = key.public().to_peer_id();
            let (transport, _) = build(&key, false).unwrap();
            let mut dialer = Swarm::new(
                transport,
                libp2p_stream::Behaviour::new(),
                dialer_id,
                libp2p::swarm::Config::with_executor(Executor::new(edge::testing::edge()))
                    .with_idle_connection_timeout(Duration::from_secs(60)),
            );
            let mut identify_requests = dialer
                .behaviour()
                .new_control()
                .accept(StreamProtocol::new(IDENTIFY))
                .unwrap();

            // Dial from a fresh port, as every node of ours does.
            dialer
                .dial(
                    DialOpts::peer_id(host_id)
                        .addresses(vec![host_addr])
                        .allocate_new_port()
                        .build(),
                )
                .unwrap();
            loop {
                if let SwarmEvent::ConnectionEstablished { .. } = dialer.select_next_some().await {
                    break;
                }
            }
            nuclei::spawn(async move {
                loop {
                    dialer.select_next_some().await;
                }
            })
            .detach();

            // The host asks the dialer who it is; the answer is held here.
            let (_, mut identify) = identify_requests.next().await.expect("an Identify request");

            let mut active_peers = Box::pin(handle.peer_addresses());
            if let Ok(early) =
                crate::rt::time::timeout(Duration::from_millis(500), &mut active_peers).await
            {
                panic!(
                    "ActivePeers answered before the dialer's Identify: {early:?}; \
                     the address it holds then is the dial's source port"
                );
            }

            let listen: Multiaddr = LISTEN.parse().unwrap();
            identify
                .write_all(&identify_reply(&key, &listen))
                .await
                .unwrap();
            identify.close().await.unwrap();

            let addresses = active_peers.await.unwrap();
            assert_eq!(addresses, vec![format!("{LISTEN}/p2p/{dialer_id}")]);

            handle.shutdown().await.unwrap();
        }),
    )
    .await
    .expect("the test finished within its deadline");
}

/// Dial the host from a fresh bare swarm and return its key and the host's
/// held Identify request stream.
async fn dial_and_hold_identify(
    host_id: libp2p::PeerId,
    host_addr: Multiaddr,
) -> (Keypair, libp2p::Stream) {
    let key = Keypair::generate_ed25519();
    let (transport, _) = build(&key, false).unwrap();
    let mut dialer = Swarm::new(
        transport,
        libp2p_stream::Behaviour::new(),
        key.public().to_peer_id(),
        libp2p::swarm::Config::with_executor(Executor::new(edge::testing::edge()))
            .with_idle_connection_timeout(Duration::from_secs(120)),
    );
    let mut identify_requests = dialer
        .behaviour()
        .new_control()
        .accept(StreamProtocol::new(IDENTIFY))
        .unwrap();
    dialer
        .dial(
            DialOpts::peer_id(host_id)
                .addresses(vec![host_addr])
                .allocate_new_port()
                .build(),
        )
        .unwrap();
    loop {
        if let SwarmEvent::ConnectionEstablished { .. } = dialer.select_next_some().await {
            break;
        }
    }
    nuclei::spawn(async move {
        loop {
            dialer.select_next_some().await;
        }
    })
    .detach();
    let (_, identify) = identify_requests.next().await.expect("an Identify request");
    (key, identify)
}

#[nuclei::test]
async fn active_peers_is_not_held_by_a_peer_that_dials_in_after_the_call() {
    crate::rt::time::timeout(
        Duration::from_secs(30),
        edge::compat(async {
            let (host, handle, _events, _replicators) =
                P2PHost::new(MockBitswapStore::new(), edge::testing::edge())
                    .await
                    .unwrap();
            nuclei::spawn(host.run()).detach();
            handle
                .listen("/ip4/127.0.0.1/tcp/0".parse().unwrap())
                .await
                .unwrap();
            let host_addr = handle.listen_addresses().await.unwrap().remove(0);
            let host_id = handle.local_peer_id_cached();

            let (key_a, mut identify_a) = dial_and_hold_identify(host_id, host_addr.clone()).await;
            let mut active_peers = Box::pin(handle.peer_addresses());
            assert!(
                crate::rt::time::timeout(Duration::from_millis(200), &mut active_peers)
                    .await
                    .is_err(),
                "held by A's Identify"
            );

            let (_key_b, _identify_b) = dial_and_hold_identify(host_id, host_addr).await;

            let listen: Multiaddr = LISTEN.parse().unwrap();
            identify_a
                .write_all(&identify_reply(&key_a, &listen))
                .await
                .unwrap();
            identify_a.close().await.unwrap();

            let answered =
                crate::rt::time::timeout(Duration::from_secs(5), &mut active_peers).await;
            assert!(
                answered.is_ok(),
                "every peer connected at the call has identified, yet ActivePeers is still \
                 held 5s later by B, which dialed in after the call"
            );

            handle.shutdown().await.unwrap();
        }),
    )
    .await
    .expect("the test finished within its deadline");
}
