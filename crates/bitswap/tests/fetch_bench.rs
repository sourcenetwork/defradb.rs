//! Fetch benchmark over loopback TCP. Run with `--ignored --nocapture --test-threads=1`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::anyhow;
use async_trait::async_trait;
use bitswap::{Bitswap, Block, Config, Store};
use bytes::Bytes;
use cid::Cid;
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::{noise, tcp, yamux, PeerId, Swarm, SwarmBuilder};
use multihash_codetable::{Code, MultihashDigest};
use tokio::sync::oneshot;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(120);
const WARMUP: usize = 2;
const MEASURED: usize = 7;
const PROTOCOLS: [&str; 3] = [
    "/ipfs/bitswap/1.2.0",
    "/ipfs/bitswap/1.1.0",
    "/ipfs/bitswap/1.0.0",
];

#[derive(Clone, Debug)]
struct MemStore(Arc<BTreeMap<Cid, Block>>);

#[async_trait]
impl Store for MemStore {
    async fn get_size(&self, cid: &Cid) -> anyhow::Result<usize> {
        self.0
            .get(cid)
            .map(|b| b.data.len())
            .ok_or_else(|| anyhow!("not found"))
    }
    async fn get(&self, cid: &Cid) -> anyhow::Result<Block> {
        self.0.get(cid).cloned().ok_or_else(|| anyhow!("not found"))
    }
    async fn has(&self, cid: &Cid) -> anyhow::Result<bool> {
        Ok(self.0.contains_key(cid))
    }
}

fn make_blocks(count: usize, size: usize, seed: usize) -> Vec<Block> {
    (0..count)
        .map(|n| {
            let mut data: Vec<u8> = (0..size)
                .map(|i| (i.wrapping_mul(31).wrapping_add(7).wrapping_add(seed)) as u8)
                .collect();
            data[..8].copy_from_slice(&((seed * 1_000_000 + n) as u64).to_le_bytes());
            let cid = Cid::new_v1(0x55, Code::Sha2_256.digest(&data));
            Block::new(Bytes::from(data), cid)
        })
        .collect()
}

fn build<S: Store>(store: S) -> (PeerId, Swarm<Bitswap<S>>) {
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();
    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(|_| Bitswap::new(peer_id, store, Config::default()))
        .unwrap()
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(120)))
        .build();
    (peer_id, swarm)
}

fn protocols() -> Vec<String> {
    PROTOCOLS.iter().map(|p| p.to_string()).collect()
}

/// A connected pair: the client swarm (driven by the caller) and the server peer id.
async fn pair(blocks: &[Block]) -> (PeerId, Swarm<Bitswap<MemStore>>) {
    let store = MemStore(Arc::new(
        blocks.iter().map(|b| (b.cid, b.clone())).collect(),
    ));
    let (server_id, mut server) = build(store);
    let (_, mut client) = build(MemStore(Arc::new(BTreeMap::new())));
    server
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
            break address;
        }
    };
    let (ready_tx, ready_rx) = oneshot::channel();
    tokio::spawn(async move {
        let mut ready = Some(ready_tx);
        loop {
            if let SwarmEvent::ConnectionEstablished { peer_id, .. } =
                server.select_next_some().await
            {
                server.behaviour_mut().on_identify(&peer_id, &protocols());
                if let Some(tx) = ready.take() {
                    let _ = tx.send(());
                }
            }
        }
    });
    client.dial(addr).unwrap();
    let client_peer = loop {
        if let SwarmEvent::ConnectionEstablished { peer_id, .. } = client.select_next_some().await {
            break peer_id;
        }
    };
    client
        .behaviour_mut()
        .on_identify(&client_peer, &protocols());
    timeout(DEADLINE, ready_rx).await.unwrap().unwrap();
    (server_id, client)
}

async fn fetch_all(
    client: &mut Swarm<Bitswap<MemStore>>,
    server: PeerId,
    blocks: &[Block],
) -> Duration {
    let start = Instant::now();
    let (_, mut rx) = client
        .behaviour_mut()
        .fetch(blocks.iter().map(|b| b.cid).collect(), vec![server]);
    let mut got = 0;
    timeout(DEADLINE, async {
        while got < blocks.len() {
            tokio::select! {
                _ = client.select_next_some() => {}
                b = rx.recv() => match b {
                    Some(_) => got += 1,
                    None => break,
                },
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out after {got}/{} blocks", blocks.len()));
    assert_eq!(got, blocks.len(), "fetch closed early");
    start.elapsed()
}

async fn bulk(count: usize, size: usize, seed: usize) -> Duration {
    let blocks = make_blocks(count, size, seed);
    let (server, mut client) = pair(&blocks).await;
    fetch_all(&mut client, server, &blocks).await
}

async fn latency(count: usize, size: usize, seed: usize) -> Vec<Duration> {
    let blocks = make_blocks(count, size, seed);
    let (server, mut client) = pair(&blocks).await;
    let mut out = Vec::with_capacity(count);
    for b in &blocks {
        out.push(fetch_all(&mut client, server, std::slice::from_ref(b)).await);
    }
    out
}

fn report_bulk(name: &str, count: usize, size: usize, mut t: Vec<Duration>) {
    t.sort();
    let (min, med, max) = (t[0], t[t.len() / 2], t[t.len() - 1]);
    let tp = |d: Duration| {
        let s = d.as_secs_f64();
        (count as f64 / s, (count * size) as f64 / 1_048_576.0 / s)
    };
    let (bps, mibs) = tp(med);
    println!(
        "{name}: min {:.1} ms, median {:.1} ms, max {:.1} ms | median {:.0} blocks/s, {:.1} MiB/s | all {:?}",
        min.as_secs_f64() * 1e3, med.as_secs_f64() * 1e3, max.as_secs_f64() * 1e3, bps, mibs,
        t.iter().map(|d| format!("{:.1}", d.as_secs_f64() * 1e3)).collect::<Vec<_>>()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn fetch_bench() {
    for (name, count, size) in [
        ("W1 2000x512B", 2000, 512),
        ("W2 64x256KiB", 64, 256 * 1024),
    ] {
        let mut t = Vec::new();
        for i in 0..WARMUP + MEASURED {
            let d = bulk(count, size, i + 1).await;
            if i >= WARMUP {
                t.push(d);
            }
        }
        report_bulk(name, count, size, t);
    }
    // W3: per iteration, the median of 200 per-fetch latencies; then min/median/max across iterations.
    let mut meds = Vec::new();
    let mut totals = Vec::new();
    for i in 0..WARMUP + MEASURED {
        let mut l = latency(200, 512, 100 + i).await;
        if i >= WARMUP {
            totals.push(l.iter().sum::<Duration>());
            l.sort();
            meds.push(l[l.len() / 2]);
        }
    }
    meds.sort();
    totals.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!(
        "W3 200 sequential 512B: per-fetch median latency min {:.3} ms, median {:.3} ms, max {:.3} ms | total-200 min {:.1} ms, median {:.1} ms, max {:.1} ms",
        ms(meds[0]), ms(meds[meds.len() / 2]), ms(meds[meds.len() - 1]),
        ms(totals[0]), ms(totals[totals.len() / 2]), ms(totals[totals.len() - 1])
    );
}
