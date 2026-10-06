#![cfg(feature = "test-utils")]

//! A partition that never closes the connection must not leave Bitswap dead
//! toward that peer after the partition heals (the `bitswap` crate's fetch path).
//!
//! Two raw swarms over loopback TCP, joined through a proxy that can stop
//! forwarding bytes. Stopping the bytes is what `docker network disconnect`
//! does: the socket stays established on both ends, nothing is reset, and
//! everything written meanwhile is delivered once the link returns. libp2p
//! drives every connection in its own task, so an unpolled swarm is not a
//! partition; only withheld bytes are.

use std::time::Duration;

use ::bitswap::{Bitswap, Block, Config as BitswapConfig};
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::{noise, tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder};
use p2p::testutil::MockBitswapStore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

const BITSWAP_PROTOCOLS: [&str; 2] = ["/ipfs/bitswap/1.2.0", "/ipfs/bitswap/1.1.0"];
/// libp2p's outbound substream request timeout: how long a want may wait for
/// the peer's half of the multistream handshake before the handler is told
/// the stream failed.
const UPGRADE_TIMEOUT: Duration = Duration::from_secs(10);

fn block(data: &[u8]) -> (cid::Cid, Vec<u8>) {
    let cid = defra_core::block::generate_cid_from_bytes(data).unwrap();
    (cid, data.to_vec())
}

fn swarm(store: MockBitswapStore) -> Swarm<Bitswap<MockBitswapStore>> {
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();
    let bitswap = Bitswap::new(peer_id, store, BitswapConfig::default());
    SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(|_| bitswap)
        .unwrap()
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(120)))
        .build()
}

async fn listen(swarm: &mut Swarm<Bitswap<MockBitswapStore>>) -> Multiaddr {
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            return address;
        }
    }
}

async fn wait_connected(swarm: &mut Swarm<Bitswap<MockBitswapStore>>) -> PeerId {
    loop {
        if let SwarmEvent::ConnectionEstablished { peer_id, .. } = swarm.select_next_some().await {
            return peer_id;
        }
    }
}

fn tcp_port(addr: &Multiaddr) -> u16 {
    addr.iter()
        .find_map(|p| match p {
            libp2p::multiaddr::Protocol::Tcp(port) => Some(port),
            _ => None,
        })
        .unwrap()
}

/// Fetches one block while driving the swarm, since `fetch` needs the
/// behaviour that only the swarm poll advances. None when the fetch gives up
/// or `deadline` passes.
async fn fetch_one(
    swarm: &mut Swarm<Bitswap<MockBitswapStore>>,
    cid: cid::Cid,
    provider: PeerId,
    deadline: Duration,
    connection_closed: &mut bool,
) -> Option<Block> {
    let (_, mut blocks) = swarm.behaviour_mut().fetch(vec![cid], vec![provider]);
    let deadline = tokio::time::Instant::now() + deadline;
    loop {
        tokio::select! {
            block = blocks.recv() => return block,
            event = swarm.select_next_some() => {
                if let SwarmEvent::ConnectionClosed { .. } = event {
                    *connection_closed = true;
                }
            }
            () = tokio::time::sleep_until(deadline) => return None,
        }
    }
}

/// Copy bytes one way, holding them while `forward` is false.
async fn pump(
    mut from: tokio::net::tcp::OwnedReadHalf,
    mut to: tokio::net::tcp::OwnedWriteHalf,
    mut forward: watch::Receiver<bool>,
) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match from.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        while !*forward.borrow() {
            if forward.changed().await.is_err() {
                return;
            }
        }
        if to.write_all(&buf[..n]).await.is_err() {
            return;
        }
    }
}

/// A single-connection TCP proxy whose byte flow can be paused in both
/// directions. Returns the address to dial and the pause switch.
async fn proxy_to(target_port: u16) -> (Multiaddr, watch::Sender<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (forward_tx, forward_rx) = watch::channel(true);
    tokio::spawn(async move {
        let (inbound, _) = listener.accept().await.unwrap();
        let outbound = TcpStream::connect(("127.0.0.1", target_port))
            .await
            .unwrap();
        inbound.set_nodelay(true).unwrap();
        outbound.set_nodelay(true).unwrap();
        let (in_read, in_write) = inbound.into_split();
        let (out_read, out_write) = outbound.into_split();
        tokio::join!(
            pump(in_read, out_write, forward_rx.clone()),
            pump(out_read, in_write, forward_rx),
        );
    });
    (
        format!("/ip4/127.0.0.1/tcp/{port}").parse().unwrap(),
        forward_tx,
    )
}

/// A want that fails on the outbound upgrade timeout must not poison the
/// peer: a later fetch over the same connection has to be served.
///
/// `drop_dead_bitswap_connection` in `dag_fetcher.rs` is the receiver-side
/// defence in depth for Go peers, whose queues can die on a failed send. It
/// hangs up on a connection this test keeps alive on purpose, so it cannot
/// make this test pass.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bitswap_recovers_after_a_partition_that_keeps_the_connection_open() {
    let (before_cid, before_data) = block(b"served before the partition");
    let (during_cid, during_data) = block(b"wanted during the partition");
    let (after_cid, after_data) = block(b"wanted after the partition heals");
    let provider_store = MockBitswapStore::new()
        .with_block(before_cid, before_data.clone())
        .with_block(during_cid, during_data)
        .with_block(after_cid, after_data.clone());

    let mut provider = swarm(provider_store);
    let mut fetcher = swarm(MockBitswapStore::new());
    let provider_id = *provider.local_peer_id();
    let fetcher_id = *fetcher.local_peer_id();

    let provider_addr = listen(&mut provider).await;
    let (proxy_addr, link) = proxy_to(tcp_port(&provider_addr)).await;
    fetcher.dial(proxy_addr).unwrap();
    let (connected_to, connected_from) =
        tokio::join!(wait_connected(&mut fetcher), wait_connected(&mut provider));
    assert_eq!(connected_to, provider_id);
    assert_eq!(connected_from, fetcher_id);

    // Identify marks the peer responsive on both sides in production; a
    // responsive peer is the state a dead queue is never repaired from.
    let protocols: Vec<String> = BITSWAP_PROTOCOLS.iter().map(|p| p.to_string()).collect();
    fetcher
        .behaviour_mut()
        .on_identify(&provider_id, &protocols);
    provider
        .behaviour_mut()
        .on_identify(&fetcher_id, &protocols);

    let mut connection_closed = false;
    let provider_task = tokio::spawn(async move {
        loop {
            provider.select_next_some().await;
        }
    });

    // Sanity: the harness serves a block through the proxy.
    let served = fetch_one(
        &mut fetcher,
        before_cid,
        provider_id,
        Duration::from_secs(10),
        &mut connection_closed,
    )
    .await
    .expect("block served before the partition");
    assert_eq!(served.data(), &before_data[..]);

    // Partition: bytes stop flowing while the fetcher has a want in flight.
    // Its outbound substream cannot finish the multistream handshake, and
    // libp2p reports the request as timed out after 10 s while the
    // connection itself stays established.
    link.send(false).unwrap();
    let hung = fetch_one(
        &mut fetcher,
        during_cid,
        provider_id,
        UPGRADE_TIMEOUT + Duration::from_secs(5),
        &mut connection_closed,
    )
    .await;
    assert!(hung.is_none(), "the partition must black-hole the want");
    assert!(
        !connection_closed,
        "a partition that keeps the socket open must not look like a disconnect"
    );

    // Heal: the held bytes are delivered and the link is live again. Nothing
    // else changes: no reconnect, no restart, no new peer, exactly as after
    // a `docker network connect`.
    link.send(true).unwrap();
    let recovered = fetch_one(
        &mut fetcher,
        after_cid,
        provider_id,
        Duration::from_secs(20),
        &mut connection_closed,
    )
    .await;
    assert!(
        !connection_closed,
        "the connection must still be the original one"
    );
    let recovered = recovered.expect(
        "no block after the heal: Bitswap stayed dead toward the peer after the upgrade timeout",
    );
    assert_eq!(recovered.data(), &after_data[..]);

    provider_task.abort();
}
