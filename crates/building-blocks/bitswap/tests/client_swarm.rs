use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bitswap::{Bitswap, Block, Config, ServerConfig, Store};
use cid::Cid;
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use nuclei::sync::mpsc::Receiver;
use nuclei::time::timeout;

mod common;
use common::block_v1;
use common::mem_store::MemStore;

const DEADLINE: Duration = Duration::from_secs(20);

type Node<S> = Swarm<Bitswap<S>>;

fn build<S: Store>(store: S, config: Config) -> (PeerId, Node<S>) {
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();
    let swarm = net::libp2p::swarm(
        &keypair,
        Bitswap::new(peer_id, store, config, edge::testing::edge()),
        Duration::from_secs(60),
        edge::testing::edge(),
    )
    .unwrap();
    (peer_id, swarm)
}

async fn listen<S: Store>(swarm: &mut Node<S>) -> Multiaddr {
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            return address;
        }
    }
}

fn serve<S: Store>(mut swarm: Node<S>) {
    nuclei::spawn(async move {
        loop {
            swarm.select_next_some().await;
        }
    })
    .detach();
}

async fn server(blocks: &[Block]) -> (PeerId, Multiaddr) {
    let (id, mut node) = build(MemStore::new(blocks), Config::default());
    let addr = listen(&mut node).await;
    serve(node);
    (id, addr)
}

async fn client_connected_to(servers: &[Multiaddr]) -> (PeerId, Node<MemStore>) {
    let (id, mut node) = build(MemStore::new(&[]), Config::default());
    for addr in servers {
        node.dial(addr.clone()).unwrap();
        timeout(DEADLINE, async {
            while !matches!(
                node.select_next_some().await,
                SwarmEvent::ConnectionEstablished { .. }
            ) {}
        })
        .await
        .unwrap();
    }
    (id, node)
}

async fn collect<S: Store>(node: &mut Node<S>, rx: &mut Receiver<Block>) -> Vec<Block> {
    let mut blocks = Vec::new();
    timeout(DEADLINE, async {
        loop {
            nuclei::select! {
                _ = node.select_next_some() => {}
                block = rx.recv() => match block {
                    Some(block) => blocks.push(block),
                    None => return,
                },
            }
        }
    })
    .await
    .expect("the fetch did not complete in time");
    blocks
}

fn cids(blocks: &[Block]) -> BTreeSet<Cid> {
    blocks.iter().map(|b| b.cid).collect()
}

#[nuclei::test(threads = 2)]
async fn fetches_twenty_blocks_from_one_provider() {
    async {
        let blocks: Vec<Block> = (0..20)
            .map(|i| block_v1(format!("block {i}").as_bytes()))
            .collect();
        let (a, a_addr) = server(&blocks).await;
        let (_, mut b) = client_connected_to(&[a_addr]).await;

        let (_, mut rx) = b
            .behaviour_mut()
            .fetch(blocks.iter().map(|x| x.cid).collect(), vec![a]);
        let got = collect(&mut b, &mut rx).await;
        assert_eq!(got.len(), 20);
        assert_eq!(cids(&got), cids(&blocks));
        for block in &got {
            assert!(blocks.contains(block));
        }
    }
    .await
}

#[nuclei::test(threads = 2)]
async fn fetches_1500_blocks_through_the_want_window() {
    async {
        let blocks: Vec<Block> = (0..1500)
            .map(|i| block_v1(format!("block {i}").as_bytes()))
            .collect();
        let (a, a_addr) = server(&blocks).await;
        let (_, mut b) = client_connected_to(&[a_addr]).await;

        let (_, mut rx) = b
            .behaviour_mut()
            .fetch(blocks.iter().map(|x| x.cid).collect(), vec![a]);
        let got = collect(&mut b, &mut rx).await;
        assert_eq!(got.len(), 1500);
        assert_eq!(cids(&got), cids(&blocks));
    }
    .await
}

#[nuclei::test(threads = 2)]
async fn present_blocks_arrive_and_absent_ones_are_skipped() {
    async {
        let present: Vec<Block> = (0..5)
            .map(|i| block_v1(format!("present {i}").as_bytes()))
            .collect();
        let absent: Vec<Block> = (0..5)
            .map(|i| block_v1(format!("absent {i}").as_bytes()))
            .collect();
        let (a, a_addr) = server(&present).await;
        let (_, mut b) = client_connected_to(&[a_addr]).await;

        let wanted = present.iter().chain(&absent).map(|x| x.cid).collect();
        let (_, mut rx) = b.behaviour_mut().fetch(wanted, vec![a]);
        let got = collect(&mut b, &mut rx).await;
        assert_eq!(cids(&got), cids(&present));
        assert_eq!(got.len(), present.len());
    }
    .await
}

#[nuclei::test(threads = 2)]
async fn falls_back_to_the_second_provider_through_have() {
    async {
        let block = block_v1(b"only the second has it");
        let (a, a_addr) = server(&[]).await;
        let (c, c_addr) = server(std::slice::from_ref(&block)).await;
        let (_, mut b) = client_connected_to(&[a_addr, c_addr]).await;

        let (_, mut rx) = b.behaviour_mut().fetch(vec![block.cid], vec![a, c]);
        let got = collect(&mut b, &mut rx).await;
        assert_eq!(got, [block]);
    }
    .await
}

#[nuclei::test(threads = 2)]
async fn no_provider_has_the_block() {
    async {
        let block = block_v1(b"nowhere");
        let (a, a_addr) = server(&[]).await;
        let (c, c_addr) = server(&[]).await;
        let (_, mut b) = client_connected_to(&[a_addr, c_addr]).await;

        let (_, mut rx) = b.behaviour_mut().fetch(vec![block.cid], vec![a, c]);
        assert!(collect(&mut b, &mut rx).await.is_empty());
    }
    .await
}

#[nuclei::test(threads = 2)]
async fn empty_inputs_close_the_channel_without_traffic() {
    async {
        let (a, a_addr) = server(&[]).await;
        let (_, mut b) = client_connected_to(&[a_addr]).await;
        let block = block_v1(b"unused");

        let (_, mut no_providers) = b.behaviour_mut().fetch(vec![block.cid], vec![]);
        let (_, mut no_cids) = b.behaviour_mut().fetch(vec![], vec![a]);
        assert!(collect(&mut b, &mut no_providers).await.is_empty());
        assert!(collect(&mut b, &mut no_cids).await.is_empty());
    }
    .await
}

#[derive(Debug, Clone)]
struct SlowStore {
    inner: MemStore,
    slow: Arc<BTreeSet<Cid>>,
    slow_gets: Arc<AtomicUsize>,
}

#[async_trait]
impl Store for SlowStore {
    async fn get_size(&self, cid: &Cid) -> bitswap::store::Result<usize> {
        self.inner.get_size(cid).await
    }

    async fn get(&self, cid: &Cid) -> bitswap::store::Result<Block> {
        if self.slow.contains(cid) {
            self.slow_gets.fetch_add(1, Ordering::SeqCst);
            nuclei::time::sleep(Duration::from_millis(40)).await;
        }
        self.inner
            .get(cid)
            .await
            .map_err(|_| bitswap::StoreError("not found".into()))
    }

    async fn has(&self, cid: &Cid) -> bitswap::store::Result<bool> {
        self.inner.has(cid).await
    }
}

#[nuclei::test(threads = 2)]
async fn cancel_closes_the_receiver_and_the_server_stops_serving() {
    async {
        let slow: Vec<Block> = (0..30)
            .map(|i| block_v1(format!("slow {i} {}", "x".repeat(2000)).as_bytes()))
            .collect();
        let marker = block_v1(b"marker");
        let all: Vec<Block> = slow.iter().cloned().chain([marker.clone()]).collect();
        let slow_gets = Arc::new(AtomicUsize::new(0));
        let store = SlowStore {
            inner: MemStore::new(&all),
            slow: Arc::new(cids(&slow)),
            slow_gets: slow_gets.clone(),
        };
        let config = Config {
            server: Some(ServerConfig {
                worker_count: 1,
                target_message_size: 1,
                ..ServerConfig::default()
            }),
            ..Config::default()
        };
        let (a, mut a_node) = build(store, config);
        let a_addr = listen(&mut a_node).await;
        serve(a_node);
        let (_, mut b) = client_connected_to(&[a_addr]).await;

        let (id, mut rx) = b
            .behaviour_mut()
            .fetch(slow.iter().map(|x| x.cid).collect(), vec![a]);
        let first = timeout(DEADLINE, async {
            loop {
                nuclei::select! {
                    _ = b.select_next_some() => {}
                    block = rx.recv() => return block,
                }
            }
        })
        .await
        .unwrap();
        assert!(first.is_some());

        assert!(b.behaviour_mut().cancel(id));
        assert!(!b.behaviour_mut().cancel(id));

        let (_, mut marker_rx) = b.behaviour_mut().fetch(vec![marker.cid], vec![a]);
        assert_eq!(collect(&mut b, &mut marker_rx).await, [marker]);
        while rx.try_recv().is_ok() {}
        assert!(collect(&mut b, &mut rx).await.is_empty());
        let served = slow_gets.load(Ordering::SeqCst);
        assert!(
            served < slow.len() / 2,
            "the server kept reading cancelled blocks: {served} of {}",
            slow.len()
        );
    }
    .await
}
