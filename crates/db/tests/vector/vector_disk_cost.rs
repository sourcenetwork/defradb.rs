//! What a vector search costs when the index lives on disk: recall@10,
//! latency and peak RSS for DiskANN (both layouts), HNSW and IVF-PQ over the
//! same corpus and the same regolith store.
//!
//! `#[ignore]`: a measurement, not a check. It runs in two steps, so the store
//! can be made cold between them:
//!
//! ```text
//! PHASE=build N=100000 DIR=/path/bench ENGINES=diskann,hnsw,ivfpq \
//!   cargo test --release -p db --test vector -- vector_disk_cost --ignored --nocapture
//! sync; echo 3 | sudo tee /proc/sys/vm/drop_caches
//! PHASE=query DIR=/path/bench ENGINE=diskann|resident|hnsw|ivfpq EFFORT=100 \
//!   PROFILE=server|embedded cargo test --release -p db --test vector -- \
//!   vector_disk_cost --ignored --nocapture
//! ```
//!
//! On macOS, copy the store with `ditto --nocache` instead of dropping caches.
//! Pass 0 of a query process is cold, and for resident codes it includes
//! loading them; later passes are warm. `EFFORT` is the engine's own knob: `L`
//! for DiskANN, `ef` for HNSW, probes for IVF-PQ; unset takes its default.
//! `THREADS=8` then measures warm queries per second with 8 searches in flight.
//!
//! The corpus is synthetic and clustered unless `CORPUS_DIR` holds `base.f32`
//! and `queries.f32` (little-endian, `DIMS` floats per vector, as
//! `ivf_real_corpus` reads them). The build writes the queries and their exact
//! top `K` next to the store, so a query process never holds the corpus and
//! its RSS is the index's.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::support::Corpus;
use db::index::vector::engine::ann::VectorIndexEngine;
use db::index::vector::engine::diskann::codec;
use db::index::vector::engine::diskann::{CodeCache, DiskAnn, DiskAnnParams};
use db::index::vector::engine::hnsw::Hnsw;
use db::index::vector::engine::ivf::pq::{IvfPq, IvfPqParams};
use db::index::vector::params::{Params, DEFAULT_M};
use db::index::vector::store::{KvNodeStore, MemoryNodeStore, Meta, Node, NodeId, VectorNodeStore};
use defra_core::vector::Metric;
use regolith::{Statistics, Ticker};
use storage::corekv::Store;
use storage::{RegolithStore, RegolithStoreOptions};

const SEED: u64 = 0xD15C_C057;
const CLUSTERS: usize = 100;
const SPREAD: f32 = 0.6;
const K: usize = 10;
const COLLECTION: u32 = 1;
/// Each engine's index is written under its own id in the one store.
const RECORDS: u32 = 1;
const RESIDENT: u32 = 2;
const HNSW: u32 = 3;
const IVFPQ: u32 = 4;

type Aux = Vec<(u8, Vec<u8>, Vec<u8>)>;

fn params(resident_codes: bool) -> DiskAnnParams {
    DiskAnnParams {
        resident_codes,
        ..Default::default()
    }
}

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn read_f32(path: &Path, dims: usize) -> Vec<Vec<f32>> {
    let bytes = std::fs::read(path).unwrap();
    let floats: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    floats.chunks_exact(dims).map(<[f32]>::to_vec).collect()
}

/// `n` corpus vectors and `q` queries: from `CORPUS_DIR` when set, else
/// clustered vectors with the queries drawn around the same centres.
fn corpus(n: usize, q: usize, dims: usize) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    match std::env::var("CORPUS_DIR") {
        Ok(dir) => {
            let dir = Path::new(&dir);
            let mut base = read_f32(&dir.join("base.f32"), dims);
            base.truncate(n);
            let mut queries = read_f32(&dir.join("queries.f32"), dims);
            queries.truncate(q);
            (base, queries)
        }
        Err(_) => {
            let mut vectors = Corpus::new(SEED).clustered(n + q, dims, CLUSTERS, SPREAD);
            let queries = vectors.split_off(n);
            (vectors, queries)
        }
    }
}

/// The exact top `K` ids per query, the queries split across every core.
fn truth(vectors: &[Vec<f32>], queries: &[Vec<f32>]) -> Vec<Vec<u64>> {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let chunk = queries.len().div_ceil(threads).max(1);
    let exact = |query: &Vec<f32>| -> Vec<u64> {
        let mut scored: Vec<(u64, f64)> = vectors
            .iter()
            .enumerate()
            .map(|(i, v)| (i as u64 + 1, Metric::Cosine.distance(query, v)))
            .collect();
        scored.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
        scored.into_iter().take(K).map(|(id, _)| id).collect()
    };
    std::thread::scope(|scope| {
        let handles: Vec<_> = queries
            .chunks(chunk)
            .map(|part| scope.spawn(move || part.iter().map(exact).collect::<Vec<_>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

async fn write(store: &RegolithStore, index: u32, nodes: &[Node], aux: &Aux, meta: Option<Meta>) {
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
    if let Some(meta) = meta {
        let mut txn = store.new_txn(false).await.unwrap();
        let mut kv = KvNodeStore::new(&mut txn, COLLECTION, index, 0);
        kv.put_meta(meta).await.unwrap();
        txn.commit().await.unwrap();
    }
}

/// Every node, every aux entry and the meta of an index built in memory.
async fn snapshot(mem: &MemoryNodeStore) -> (Vec<Node>, Aux, Option<Meta>) {
    let mut nodes = Vec::new();
    mem.iterate_nodes(|node| {
        nodes.push(node);
        Ok(())
    })
    .await
    .unwrap();
    let mut aux = Vec::new();
    for kind in 0..=u8::MAX {
        mem.iterate_aux(kind, b"", |key, value| {
            aux.push((kind, key.to_vec(), value.to_vec()));
            Ok(())
        })
        .await
        .unwrap();
    }
    (nodes, aux, mem.get_meta().await.unwrap())
}

/// Writes the graph in both layouts: records that carry their neighbours'
/// codes, and records of ids only with every code kept once beside them.
async fn build_diskann(vectors: &[Vec<f32>], store: &RegolithStore) {
    let start = Instant::now();
    let mut engine =
        DiskAnn::try_new(MemoryNodeStore::new(), Metric::Cosine, params(false), 7).unwrap();
    for (i, v) in vectors.iter().enumerate() {
        engine.insert(NodeId(i as u64 + 1), v).await.unwrap();
    }
    while engine.should_build().await.unwrap() {
        engine.build().await.unwrap();
    }
    let m = engine.state().await.unwrap().unwrap().m as usize;
    eprintln!("BUILD diskann m={m} in {:.1?}", start.elapsed());

    let (nodes, aux, _) = snapshot(engine.store()).await;
    drop(engine);
    let (records, shared): (Aux, Aux) = aux
        .into_iter()
        .partition(|(kind, _, _)| *kind == codec::GRAPH);
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
    let graph_mib = |aux: &Aux| {
        aux.iter()
            .filter(|(kind, _, _)| *kind == codec::GRAPH)
            .map(|(_, _, v)| v.len())
            .sum::<usize>()
            >> 20
    };
    eprintln!(
        "RECORDS with codes={} MiB ids only={} MiB resident codes={} MiB",
        graph_mib(&records),
        graph_mib(&resident),
        (vectors.len() * m) >> 20
    );

    let mut with_codes = shared;
    with_codes.extend(records);
    write(store, RECORDS, &nodes, &with_codes, None).await;
    write(store, RESIDENT, &nodes, &resident, None).await;
}

async fn build_hnsw(vectors: &[Vec<f32>], store: &RegolithStore) {
    let start = Instant::now();
    let mut engine = Hnsw::new(
        MemoryNodeStore::new(),
        Metric::Cosine,
        Params::new(DEFAULT_M),
        7,
    );
    for (i, v) in vectors.iter().enumerate() {
        engine.insert(NodeId(i as u64 + 1), v).await.unwrap();
    }
    eprintln!("BUILD hnsw in {:.1?}", start.elapsed());
    let (nodes, aux, meta) = snapshot(engine.store()).await;
    write(store, HNSW, &nodes, &aux, meta).await;
}

async fn build_ivfpq(vectors: &[Vec<f32>], store: &RegolithStore) {
    let start = Instant::now();
    let mut engine = IvfPq::try_new(
        MemoryNodeStore::new(),
        Metric::Cosine,
        IvfPqParams::default(),
        7,
    )
    .unwrap();
    for (i, v) in vectors.iter().enumerate() {
        engine.insert(NodeId(i as u64 + 1), v).await.unwrap();
    }
    engine.build().await.unwrap();
    eprintln!("BUILD ivfpq in {:.1?}", start.elapsed());
    let (nodes, aux, meta) = snapshot(engine.store()).await;
    write(store, IVFPQ, &nodes, &aux, meta).await;
}

async fn build(n: usize, q: usize, dims: usize, dir: &Path) {
    let (vectors, queries) = corpus(n, q, dims);
    let start = Instant::now();
    let truth = truth(&vectors, &queries);
    eprintln!(
        "TRUTH {} queries over {}x{dims} in {:.1?}",
        queries.len(),
        vectors.len(),
        start.elapsed()
    );
    if let Ok(published) = std::env::var("CORPUS_DIR").map(|d| Path::new(&d).join("gt10.u64")) {
        if let Ok(bytes) = std::fs::read(published) {
            let ids: Vec<u64> = bytes
                .as_chunks::<8>()
                .0
                .iter()
                .map(|b| u64::from_le_bytes(*b))
                .collect();
            let agree: usize = ids
                .as_chunks::<K>()
                .0
                .iter()
                .zip(&truth)
                .map(|(theirs, ours)| ours.iter().filter(|id| theirs.contains(id)).count())
                .sum();
            eprintln!(
                "TRUTH agrees with the published top {K} on {:.4}",
                agree as f64 / (K * truth.len()) as f64
            );
        }
    }
    let floats: Vec<u8> = queries
        .iter()
        .flatten()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    std::fs::write(dir.join("queries.f32"), floats).unwrap();
    let ids: Vec<u8> = truth
        .iter()
        .flatten()
        .flat_map(|id| id.to_le_bytes())
        .collect();
    std::fs::write(dir.join("truth.u64"), ids).unwrap();
    std::fs::write(dir.join("dims"), dims.to_string()).unwrap();

    let store =
        RegolithStore::open_with_options(dir.join("db"), RegolithStoreOptions::new()).unwrap();
    for engine in env("ENGINES", "diskann,hnsw,ivfpq").split(',') {
        match engine {
            "diskann" => build_diskann(&vectors, &store).await,
            "hnsw" => build_hnsw(&vectors, &store).await,
            "ivfpq" => build_ivfpq(&vectors, &store).await,
            other => panic!("unknown engine {other}"),
        }
    }
    store.close().await.ok();
}

/// Peak resident set in MiB from `VmHWM` on Linux; elsewhere only the current
/// RSS from `ps`, which the label says.
fn rss() -> String {
    let peak_kib = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|l| l.strip_prefix("VmHWM:"))
                .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
        });
    if let Some(kib) = peak_kib {
        return format!("peak={}MiB", kib >> 10);
    }
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    let kib: u64 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0);
    format!("current={}MiB", kib >> 10)
}

/// Builds `engine`'s index over `txn` as `$e` and evaluates `$body` with it;
/// `codes` is the resident-codes cache every search on the store shares.
macro_rules! with_engine {
    ($engine:expr, $txn:expr, $codes:expr, $e:ident => $body:expr) => {
        match $engine {
            "diskann" => {
                let kv = KvNodeStore::new($txn, COLLECTION, RECORDS, 0);
                let $e = DiskAnn::try_new(kv, Metric::Cosine, params(false), 7).unwrap();
                $body
            }
            "resident" => {
                let kv = KvNodeStore::new($txn, COLLECTION, RESIDENT, 0);
                let $e = DiskAnn::try_new(kv, Metric::Cosine, params(true), 7)
                    .unwrap()
                    .with_codes($codes.clone());
                $body
            }
            "hnsw" => {
                let kv = KvNodeStore::new($txn, COLLECTION, HNSW, 0);
                let $e = Hnsw::new(kv, Metric::Cosine, Params::new(DEFAULT_M), 7);
                $body
            }
            "ivfpq" => {
                let kv = KvNodeStore::new($txn, COLLECTION, IVFPQ, 0);
                let $e = IvfPq::try_new(kv, Metric::Cosine, IvfPqParams::default(), 7).unwrap();
                $body
            }
            other => panic!("unknown engine {other}"),
        }
    };
}

struct Run {
    label: String,
    effort: Option<usize>,
    passes: usize,
    queries: Vec<Vec<f32>>,
    truth: Vec<Vec<u64>>,
    stats: Arc<Statistics>,
}

impl Run {
    async fn measure<E>(&self, engine: &E)
    where
        E: VectorIndexEngine,
    {
        let q = self.queries.len();
        let effort = self
            .effort
            .map_or_else(|| "default".to_string(), |e| e.to_string());
        for pass in 0..self.passes {
            self.stats.reset();
            let mut latencies = Vec::with_capacity(q);
            let mut hits = 0;
            for (query, want) in self.queries.iter().zip(&self.truth) {
                let start = Instant::now();
                let got = engine
                    .search(query.as_slice(), K, self.effort)
                    .await
                    .unwrap();
                latencies.push(start.elapsed().as_nanos());
                hits += got.iter().filter(|n| want.contains(&n.id.0)).count();
            }
            let total: u128 = latencies.iter().sum();
            latencies.sort();
            let ms = |nanos: u128| nanos as f64 / 1e6;
            eprintln!(
                "RESULT {} effort={effort} pass{pass}: recall@{K}={:.4} mean={:.2}ms \
                 p50={:.2}ms p99={:.2}ms cache_misses={}",
                self.label,
                hits as f64 / (K * q) as f64,
                ms(total) / q as f64,
                ms(latencies[q / 2]),
                ms(latencies[q * 99 / 100]),
                self.stats.get_ticker(Ticker::BlockCacheMiss) / q as u64,
            );
        }
        eprintln!("RSS {} {}", self.label, rss());
    }

    /// Queries per second with `threads` searches in flight, each thread on
    /// its own read transaction and runtime, after the latency passes.
    fn throughput(
        &self,
        store: &RegolithStore,
        engine: &str,
        codes: &Arc<CodeCache>,
        threads: usize,
    ) {
        let start = Instant::now();
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .build()
                        .unwrap();
                    runtime.block_on(async {
                        let mut txn = store.new_txn(true).await.unwrap();
                        with_engine!(engine, &mut txn, codes, e => {
                            for query in &self.queries {
                                e.search(query.as_slice(), K, self.effort).await.unwrap();
                            }
                        });
                        txn.discard();
                    });
                });
            }
        });
        let qps = (threads * self.queries.len()) as f64 / start.elapsed().as_secs_f64();
        eprintln!(
            "QPS {} effort={} threads={threads}: {qps:.0}",
            self.label,
            self.effort
                .map_or_else(|| "default".to_string(), |e| e.to_string()),
        );
    }
}

async fn query(dir: &Path, engine: &str, profile: &str, effort: Option<usize>, passes: usize) {
    let dims: usize = std::fs::read_to_string(dir.join("dims"))
        .unwrap()
        .parse()
        .unwrap();
    let ids: Vec<u64> = std::fs::read(dir.join("truth.u64"))
        .unwrap()
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| u64::from_le_bytes(*b))
        .collect();
    let stats = Arc::new(Statistics::new());
    let run = Run {
        label: format!("{profile}/{engine}"),
        effort,
        passes,
        queries: read_f32(&dir.join("queries.f32"), dims),
        truth: ids.as_chunks::<K>().0.iter().map(|t| t.to_vec()).collect(),
        stats: stats.clone(),
    };

    let mut options = match profile {
        "server" => RegolithStoreOptions::new(),
        _ => RegolithStoreOptions::embedded(),
    };
    options.engine = options.engine.statistics(Some(stats));
    let store = RegolithStore::open_with_options(dir.join("db"), options).unwrap();
    let codes = Arc::new(CodeCache::default());
    let mut txn = store.new_txn(true).await.unwrap();
    with_engine!(engine, &mut txn, codes, e => run.measure(&e).await);
    txn.discard();
    let threads: usize = env("THREADS", "0").parse().unwrap();
    if threads > 0 {
        run.throughput(&store, engine, &codes, threads);
    }
    store.close().await.ok();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn vector_disk_cost() {
    let dir = std::path::PathBuf::from(env("DIR", "target/vector-disk-cost"));
    std::fs::create_dir_all(&dir).unwrap();
    match env("PHASE", "query").as_str() {
        "build" => {
            build(
                env("N", "100000").parse().unwrap(),
                env("Q", "200").parse().unwrap(),
                env("DIMS", "384").parse().unwrap(),
                &dir,
            )
            .await
        }
        _ => {
            query(
                &dir,
                &env("ENGINE", "diskann"),
                &env("PROFILE", "server"),
                std::env::var("EFFORT")
                    .ok()
                    .filter(|e| !e.is_empty())
                    .map(|e| e.parse().unwrap()),
                env("PASSES", "2").parse().unwrap(),
            )
            .await
        }
    }
}
