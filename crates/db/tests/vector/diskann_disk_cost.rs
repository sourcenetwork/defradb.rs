//! Whether DiskANN search is disk-bound: records that carry their neighbours'
//! PQ codes, against resident codes, where records hold only neighbour ids
//! and a search reads codes from memory.
//!
//! `#[ignore]`: a measurement, not a check. It runs in two steps, so the store
//! can be made cold between them:
//!
//! ```text
//! PHASE=build N=100000 DIR=/path/bench \
//!   cargo test --release -p db --test vector -- diskann_disk_cost --ignored --nocapture
//! ditto --nocache /path/bench /path/cold
//! PHASE=query DIR=/path/cold LAYOUT=records|resident PROFILE=server|embedded \
//!   cargo test --release -p db --test vector -- diskann_disk_cost --ignored --nocapture
//! ```
//!
//! `ditto --nocache` copies without populating the OS cache, so each query
//! process starts cold; pass 0 is cold, and for resident codes it includes
//! loading them. Later passes are warm.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::support::Corpus;
use db::index::vector::engine::ann::VectorIndexEngine;
use db::index::vector::engine::diskann::codec;
use db::index::vector::engine::diskann::{CodeCache, DiskAnn, DiskAnnParams};
use db::index::vector::store::{KvNodeStore, MemoryNodeStore, Node, NodeId, VectorNodeStore};
use defra_core::vector::Metric;
use regolith::{Statistics, Ticker};
use storage::corekv::Store;
use storage::{RegolithStore, RegolithStoreOptions};

const SEED: u64 = 0xD15C_C057;
const D: usize = 384;
const CLUSTERS: usize = 100;
const SPREAD: f32 = 0.6;
const Q: usize = 200;
const K: usize = 10;
const COLLECTION: u32 = 1;
/// The same graph is written once per layout, under its own index id.
const RECORDS: u32 = 1;
const RESIDENT: u32 = 2;

fn params(resident_codes: bool) -> DiskAnnParams {
    DiskAnnParams {
        r: 64,
        l_build: 64,
        l_search: 100,
        resident_codes,
        ..Default::default()
    }
}

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// `n` corpus vectors followed by `Q` queries drawn around the same centres.
fn corpus(n: usize) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let mut vectors = Corpus::new(SEED).clustered(n + Q, D, CLUSTERS, SPREAD);
    let queries = vectors.split_off(n);
    (vectors, queries)
}

async fn write(store: &RegolithStore, index: u32, nodes: &[Node], aux: &[(u8, Vec<u8>, Vec<u8>)]) {
    for batch in nodes.chunks(5_000) {
        let mut txn = store.new_txn(false).await.unwrap();
        let mut kv = KvNodeStore::new(&mut txn, COLLECTION, index, 0);
        for node in batch {
            kv.put_node(node.clone()).await.unwrap();
        }
        txn.commit().await.unwrap();
    }
    for batch in aux.chunks(5_000) {
        let mut txn = store.new_txn(false).await.unwrap();
        let mut kv = KvNodeStore::new(&mut txn, COLLECTION, index, 0);
        for (kind, key, value) in batch {
            kv.put_aux(*kind, key, value).await.unwrap();
        }
        txn.commit().await.unwrap();
    }
}

/// Builds once in memory, then writes the graph in both layouts under `dir`.
async fn build(n: usize, dir: &Path) {
    let (vectors, _) = corpus(n);
    let start = Instant::now();
    let mut engine =
        DiskAnn::try_new(MemoryNodeStore::new(), Metric::Cosine, params(false), 7).unwrap();
    for (i, v) in vectors.iter().enumerate() {
        engine.insert(NodeId(i as u64 + 1), v).await.unwrap();
    }
    drop(vectors);
    while engine.should_build().await.unwrap() {
        engine.build().await.unwrap();
    }
    let m = engine.state().await.unwrap().unwrap().m as usize;
    eprintln!("BUILD {n}x{D} m={m} in {:?}", start.elapsed());

    let mem = engine.store();
    let mut nodes: Vec<Node> = Vec::with_capacity(n);
    mem.iterate_nodes(|node| {
        nodes.push(node);
        Ok(())
    })
    .await
    .unwrap();
    let mut shared = Vec::new();
    let mut records = Vec::new();
    for kind in [
        codec::TRAINED,
        codec::GRAPH_STATE,
        codec::CODEBOOK,
        codec::GRAPH,
    ] {
        mem.iterate_aux(kind, b"", |key, value| {
            let entry = (kind, key.to_vec(), value.to_vec());
            match kind {
                codec::GRAPH => records.push(entry),
                _ => shared.push(entry),
            }
            Ok(())
        })
        .await
        .unwrap();
    }
    let mut resident = shared.clone();
    for (_, key, value) in &records {
        let record = codec::decode_record(value, m).unwrap();
        resident.push((
            codec::GRAPH,
            key.clone(),
            codec::encode_ids(&record.neighbors),
        ));
        resident.push((codec::CODE, key.clone(), record.code));
    }
    let record_bytes: usize = records.iter().map(|(_, _, v)| v.len()).sum();
    let ids_bytes: usize = resident
        .iter()
        .filter(|(kind, _, _)| *kind == codec::GRAPH)
        .map(|(_, _, v)| v.len())
        .sum();
    eprintln!(
        "RECORDS with codes={} MiB ids only={} MiB resident codes={} MiB",
        record_bytes >> 20,
        ids_bytes >> 20,
        (n * m) >> 20
    );

    shared.extend(records);
    let store =
        RegolithStore::open_with_options(dir.join("db"), RegolithStoreOptions::new()).unwrap();
    write(&store, RECORDS, &nodes, &shared).await;
    write(&store, RESIDENT, &nodes, &resident).await;
    store.close().await.ok();
    std::fs::write(dir.join("n"), n.to_string()).unwrap();
}

async fn query(dir: &Path, layout: &str, profile: &str, passes: usize) {
    let n = std::fs::read_to_string(dir.join("n"))
        .unwrap()
        .parse()
        .unwrap();
    let (_, queries) = corpus(n);

    let stats = Arc::new(Statistics::new());
    let mut options = match profile {
        "server" => RegolithStoreOptions::new(),
        _ => RegolithStoreOptions::embedded(),
    };
    options.engine.statistics = Some(stats.clone());
    let store = RegolithStore::open_with_options(dir.join("db"), options).unwrap();
    let mut txn = store.new_txn(true).await.unwrap();
    let resident = layout == "resident";
    let index = if resident { RESIDENT } else { RECORDS };
    let kv = KvNodeStore::new(&mut txn, COLLECTION, index, 0);
    let mut engine = DiskAnn::try_new(kv, Metric::Cosine, params(resident), 7).unwrap();
    if resident {
        engine = engine.with_codes(Arc::new(CodeCache::default()));
    }

    for pass in 0..passes {
        stats.reset();
        let mut latencies = Vec::with_capacity(Q);
        for q in &queries {
            let start = Instant::now();
            engine.search(q.as_slice(), K, None).await.unwrap();
            latencies.push(start.elapsed().as_nanos());
        }
        let total: u128 = latencies.iter().sum();
        latencies.sort();
        let ms = |nanos: u128| nanos as f64 / 1e6;
        eprintln!(
            "RESULT {profile:8} {layout:8} pass{pass}: mean={:.2}ms p50={:.2}ms p99={:.2}ms \
             cache_misses={}",
            ms(total) / Q as f64,
            ms(latencies[Q / 2]),
            ms(latencies[Q * 99 / 100]),
            stats.get_ticker(Ticker::BlockCacheMiss) / Q as u64,
        );
    }
    drop(engine);
    txn.discard();
    store.close().await.ok();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn diskann_disk_cost() {
    let dir = std::path::PathBuf::from(env("DIR", "target/diskann-disk-cost"));
    std::fs::create_dir_all(&dir).unwrap();
    match env("PHASE", "query").as_str() {
        "build" => build(env("N", "100000").parse().unwrap(), &dir).await,
        _ => {
            let passes = env("PASSES", "2").parse().unwrap();
            query(
                &dir,
                &env("LAYOUT", "records"),
                &env("PROFILE", "server"),
                passes,
            )
            .await
        }
    }
}
