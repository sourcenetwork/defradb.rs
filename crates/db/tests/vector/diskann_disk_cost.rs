//! Whether DiskANN search is disk-bound: the shipped layout, where each graph
//! record carries its neighbours' PQ codes, against a resident-code layout,
//! where records hold only neighbour ids and every code stays in memory.
//!
//! `#[ignore]`: a measurement, not a check. It runs in two steps, so the store
//! can be made cold between them:
//!
//! ```text
//! PHASE=build N=100000 DIR=/path/bench \
//!   cargo test --release -p db --test vector -- diskann_disk_cost --ignored --nocapture
//! ditto --nocache /path/bench /path/cold
//! PHASE=query DIR=/path/cold LAYOUT=engine|records|resident PROFILE=server|embedded \
//!   cargo test --release -p db --test vector -- diskann_disk_cost --ignored --nocapture
//! ```
//!
//! `ditto --nocache` copies without populating the OS cache, so each query
//! process starts cold; pass 0 is cold and later passes are warm.
//!
//! The resident layout does not exist in the engine, so both `records` and
//! `resident` run a stand-in walk and re-rank that differ only in where codes
//! come from. `LAYOUT=engine` runs the engine's own search over the shipped
//! layout; it should track `records`, and if it stops doing so the stand-in has
//! drifted from the real search.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::support::Corpus;
use db::index::vector::engine::ann::VectorIndexEngine;
use db::index::vector::engine::diskann::codec;
use db::index::vector::engine::diskann::{DiskAnn, DiskAnnParams};
use db::index::vector::engine::ivf;
use db::index::vector::quantize::{ProductQuantizer, Quantizer};
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
const L: usize = 100;
/// Ids-only records for the resident layout, beside the shipped ones.
const IDS_ONLY: u8 = b'L';
const COLLECTION: u32 = 1;
const INDEX: u32 = 1;

#[derive(Default)]
struct Tally {
    nanos: u128,
    record_reads: u64,
    record_bytes: u64,
    node_reads: u64,
}

fn decode_ids(bytes: &[u8]) -> impl Iterator<Item = NodeId> + '_ {
    bytes[4..]
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| NodeId(u64::from_le_bytes(*c)))
}

/// A stand-in for the engine's walk and re-rank, reading codes from records,
/// or from memory when `codes` is given.
async fn search<S: VectorNodeStore>(
    store: &S,
    pq: &ProductQuantizer,
    m: usize,
    codes: Option<&[Vec<u8>]>,
    entry: NodeId,
    query: &[f32],
    t: &mut Tally,
) {
    let start = Instant::now();
    let table = pq.distance_table(query);
    let resident = |id: NodeId| codes.unwrap()[id.0 as usize].as_slice();
    let expand = async |id: NodeId, t: &mut Tally| -> Vec<(NodeId, f64)> {
        t.record_reads += 1;
        let key = codec::record_key(id);
        if codes.is_some() {
            let bytes = store.get_aux(IDS_ONLY, &key).await.unwrap().unwrap();
            t.record_bytes += bytes.len() as u64;
            decode_ids(&bytes)
                .map(|n| (n, pq.distance(&table, resident(n))))
                .collect()
        } else {
            let bytes = store.get_aux(codec::GRAPH, &key).await.unwrap().unwrap();
            t.record_bytes += bytes.len() as u64;
            codec::decode_record(&bytes, m)
                .unwrap()
                .neighbors
                .into_iter()
                .map(|(n, c)| (n, pq.distance(&table, &c)))
                .collect()
        }
    };
    let entry_distance = match codes {
        Some(_) => pq.distance(&table, resident(entry)),
        None => {
            let key = codec::record_key(entry);
            let bytes = store.get_aux(codec::GRAPH, &key).await.unwrap().unwrap();
            pq.distance(&table, &codec::decode_record(&bytes, m).unwrap().code)
        }
    };
    let mut frontier = vec![(entry, entry_distance)];
    let mut seen = HashSet::from([entry]);
    let mut expanded = HashSet::new();
    let mut visited = Vec::new();
    while let Some(&(id, d)) = frontier.iter().find(|(id, _)| !expanded.contains(id)) {
        expanded.insert(id);
        visited.push((id, d));
        for (n, nd) in expand(id, t).await {
            if seen.insert(n) {
                frontier.push((n, nd));
            }
        }
        frontier.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        frontier.truncate(L);
    }
    visited.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut ranked = Vec::with_capacity(L);
    for (id, _) in visited.into_iter().take(L) {
        t.node_reads += 1;
        let node = store.get_node(id).await.unwrap().unwrap();
        ranked.push(Metric::Cosine.distance_stored(query, &node.vector));
    }
    ranked.sort_by(f64::total_cmp);
    ranked.truncate(K);
    t.nanos += start.elapsed().as_nanos();
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

/// Builds in memory, then writes nodes, the shipped records, the ids-only
/// records and the resident codes under `dir`.
fn params() -> DiskAnnParams {
    DiskAnnParams {
        r: 64,
        l_build: 64,
        l_search: L as u32,
        ..Default::default()
    }
}

async fn build(n: usize, dir: &Path) {
    let params = params();
    let (vectors, _) = corpus(n);
    let start = Instant::now();
    let mut engine = DiskAnn::try_new(MemoryNodeStore::new(), Metric::Cosine, params, 7).unwrap();
    for (i, v) in vectors.iter().enumerate() {
        engine.insert(NodeId(i as u64 + 1), v).await.unwrap();
    }
    drop(vectors);
    while engine.should_build().await.unwrap() {
        engine.build().await.unwrap();
    }
    let state = engine.state().await.unwrap().unwrap();
    let m = state.m as usize;
    eprintln!(
        "BUILD {n}x{D} m={m} R={} in {:?}",
        params.r,
        start.elapsed()
    );

    let mem = engine.store();
    let mut nodes: Vec<Node> = Vec::with_capacity(n);
    mem.iterate_nodes(|node| {
        nodes.push(node);
        Ok(())
    })
    .await
    .unwrap();
    let mut aux: Vec<(u8, Vec<u8>, Vec<u8>)> = Vec::new();
    for kind in [
        codec::TRAINED,
        codec::GRAPH_STATE,
        codec::CODEBOOK,
        codec::GRAPH,
    ] {
        mem.iterate_aux(kind, b"", |k, v| {
            aux.push((kind, k.to_vec(), v.to_vec()));
            Ok(())
        })
        .await
        .unwrap();
    }
    let mut codes = vec![0u8; (n + 1) * m];
    let mut ids_only = Vec::with_capacity(n);
    let (mut record_bytes, mut ids_only_bytes) = (0, 0);
    for (kind, k, v) in &aux {
        if *kind != codec::GRAPH {
            continue;
        }
        let record = codec::decode_record(v, m).unwrap();
        let id = codec::id_from_key(k).unwrap().0 as usize;
        codes[id * m..(id + 1) * m].copy_from_slice(&record.code);
        let mut ids = (record.neighbors.len() as u32).to_le_bytes().to_vec();
        for (neighbor, _) in &record.neighbors {
            ids.extend_from_slice(&neighbor.0.to_le_bytes());
        }
        record_bytes += v.len();
        ids_only_bytes += ids.len();
        ids_only.push((k.clone(), ids));
    }
    eprintln!(
        "RECORDS shipped={} MiB ids-only={} MiB resident codes={} MiB",
        record_bytes >> 20,
        ids_only_bytes >> 20,
        (n * m) >> 20
    );

    let store =
        RegolithStore::open_with_options(dir.join("db"), RegolithStoreOptions::new()).unwrap();
    for batch in nodes.chunks(5_000) {
        let mut txn = store.new_txn(false).await.unwrap();
        let mut kv = KvNodeStore::new(&mut txn, COLLECTION, INDEX, 0);
        for node in batch {
            kv.put_node(node.clone()).await.unwrap();
        }
        txn.commit().await.unwrap();
    }
    let ids_only = ids_only.into_iter().map(|(k, v)| (IDS_ONLY, k, v));
    let records: Vec<_> = aux.into_iter().chain(ids_only).collect();
    for batch in records.chunks(5_000) {
        let mut txn = store.new_txn(false).await.unwrap();
        let mut kv = KvNodeStore::new(&mut txn, COLLECTION, INDEX, 0);
        for (kind, k, v) in batch {
            kv.put_aux(*kind, k, v).await.unwrap();
        }
        txn.commit().await.unwrap();
    }
    store.close().await.ok();

    let mut meta = (n as u64).to_le_bytes().to_vec();
    meta.extend_from_slice(&(m as u32).to_le_bytes());
    meta.extend_from_slice(&state.entry.unwrap().0.to_le_bytes());
    std::fs::write(dir.join("meta"), meta).unwrap();
    std::fs::write(dir.join("codes"), codes).unwrap();
}

async fn query(dir: &Path, layout: &str, profile: &str, passes: usize) {
    let meta = std::fs::read(dir.join("meta")).unwrap();
    let n = u64::from_le_bytes(meta[0..8].try_into().unwrap()) as usize;
    let m = u32::from_le_bytes(meta[8..12].try_into().unwrap()) as usize;
    let entry = NodeId(u64::from_le_bytes(meta[12..20].try_into().unwrap()));
    let codes: Option<Vec<Vec<u8>>> = (layout == "resident").then(|| {
        let bytes = std::fs::read(dir.join("codes")).unwrap();
        bytes.chunks_exact(m).map(<[u8]>::to_vec).collect()
    });
    let (_, queries) = corpus(n);
    let queries: Vec<Vec<f32>> = queries.iter().map(|q| Metric::Cosine.prepare(q)).collect();

    let stats = Arc::new(Statistics::new());
    let mut options = match profile {
        "server" => RegolithStoreOptions::new(),
        _ => RegolithStoreOptions::embedded(),
    };
    options.engine.statistics = Some(stats.clone());
    let store = RegolithStore::open_with_options(dir.join("db"), options).unwrap();
    let mut txn = store.new_txn(true).await.unwrap();
    let kv = KvNodeStore::new(&mut txn, COLLECTION, INDEX, 0);
    let mut books = Vec::new();
    kv.iterate_aux(codec::CODEBOOK, b"", |_, v| {
        books.push(ivf::decode_centroids(v)?);
        Ok(())
    })
    .await
    .unwrap();
    let pq = ProductQuantizer::from_books(D, books).unwrap();
    let engine = DiskAnn::try_new(kv, Metric::Cosine, params(), 7).unwrap();

    for pass in 0..passes {
        stats.reset();
        let mut t = Tally::default();
        let mut latencies = Vec::with_capacity(Q);
        for q in &queries {
            let before = t.nanos;
            if layout == "engine" {
                let start = Instant::now();
                engine.search(q.as_slice(), K, None).await.unwrap();
                t.nanos += start.elapsed().as_nanos();
            } else {
                let store = engine.store();
                search(store, &pq, m, codes.as_deref(), entry, q, &mut t).await;
            }
            latencies.push(t.nanos - before);
        }
        latencies.sort();
        let ms = |nanos: u128| nanos as f64 / 1e6;
        let per_query = |total: u64| total / Q as u64;
        eprintln!(
            "RESULT {profile:8} {layout:8} pass{pass}: mean={:.2}ms p50={:.2}ms p99={:.2}ms \
             hops={} record_KiB={} node_reads={} cache_misses={}",
            ms(t.nanos) / Q as f64,
            ms(latencies[Q / 2]),
            ms(latencies[Q * 99 / 100]),
            per_query(t.record_reads),
            per_query(t.record_bytes) / 1024,
            per_query(t.node_reads),
            per_query(stats.get_ticker(Ticker::BlockCacheMiss)),
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
