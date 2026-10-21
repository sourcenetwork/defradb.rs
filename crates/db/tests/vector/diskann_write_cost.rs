//! Insert throughput into a trained DiskANN index through `VectorIndex` on an
//! on-disk regolith store, at several concurrent writers: what each pending
//! insert costs, and whether writers conflict.
//!
//! Ignored; run on the bench machine:
//!
//! ```text
//! DIMS=768 INSERTS=4000 WRITERS=1,4,16,64 TMPDIR=/data/tmp \
//!   cargo test --release -p db --test vector diskann_write_cost -- --ignored --nocapture
//! ```
//!
//! Prints one `WRITE` line per writer count: commits per second, conflicts per
//! commit, and p50 and p99 commit latency.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::support::Corpus;
use db::index::vector::engine::diskann::CodeCaches;
use db::index::vector::engine::diskann::TRAIN_THRESHOLD;
use db::index::vector::index::VectorIndex;
use document::NormalValue;
use schema::{
    DistanceMetric, IndexDescription, IndexedFieldDescription, VectorAlgorithm,
    VectorIndexDescription,
};
use storage::corekv::{Error, Store};
use storage::index::CollectionIndex;
use storage::RegolithStore;

const COLLECTION: u32 = 41;

/// Worker threads the writers are spread over.
const THREADS: usize = 8;

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn description(dimensions: usize) -> IndexDescription {
    IndexDescription {
        name: "by_embedding".to_string(),
        id: 1,
        fields: vec![IndexedFieldDescription {
            name: "embedding".to_string(),
            descending: false,
        }],
        unique: false,
        kind: None,
        auto_generated: false,
    }
    .as_vector(VectorIndexDescription {
        algorithm: VectorAlgorithm::DiskAnn,
        metric: DistanceMetric::Cosine,
        dimensions: dimensions as u32,
        hnsw: None,
        ivfpq: None,
        ivfflat: None,
        ssg: None,
        diskann: Some(schema::DiskAnnParams::default()),
    })
}

fn value(vector: Vec<f32>) -> Vec<NormalValue> {
    vec![NormalValue::Float32Array(vector)]
}

fn is_conflict(error: &Error) -> bool {
    matches!(
        error,
        Error::TxnConflict | Error::TxnConflictDetails(_) | Error::TxnRetryExhausted(_)
    )
}

/// A store whose index has trained, so every later insert is a pending one.
async fn trained(dir: &std::path::Path, dimensions: usize) -> (RegolithStore, Arc<VectorIndex>) {
    let store = RegolithStore::open(dir).unwrap();
    let mut index = VectorIndex::try_new(COLLECTION, description(dimensions)).unwrap();
    index.share_quantizer(&CodeCaches::default());
    let index = Arc::new(index);
    let mut corpus = Corpus::new(0x7121);
    let mut txn = store.new_txn(false).await.unwrap();
    for id in 1..=TRAIN_THRESHOLD {
        index
            .save(&mut txn, id, &value(corpus.vector(dimensions)))
            .await
            .unwrap();
    }
    txn.commit().await.unwrap();
    (store, index)
}

struct Run {
    commits: u64,
    conflicts: u64,
    latencies: Vec<Duration>,
    elapsed: Duration,
}

async fn run(dimensions: usize, writers: u64, inserts: u64) -> Run {
    let dir = tempfile::tempdir().unwrap();
    let (store, index) = trained(dir.path(), dimensions).await;
    let next = Arc::new(AtomicU64::new(TRAIN_THRESHOLD + 1));
    let last = TRAIN_THRESHOLD + inserts;
    let pool = workers::Pool::start(workers::Config::unplaced(THREADS)).unwrap();
    let started = Instant::now();
    let mut tasks = Vec::with_capacity(writers as usize);
    for writer in 0..writers {
        let (store, index, next) = (store.clone(), index.clone(), next.clone());
        let target = workers::Target::Worker(writer as usize % THREADS);
        tasks.push(
            pool.submit(target, move || async move {
                let mut corpus = Corpus::new(0xA000 + writer);
                let (mut conflicts, mut latencies) = (0, Vec::new());
                loop {
                    let id = next.fetch_add(1, Ordering::Relaxed);
                    if id > last {
                        return (conflicts, latencies);
                    }
                    let vector = corpus.vector(dimensions);
                    loop {
                        let begun = Instant::now();
                        let mut txn = store.new_txn(false).await.unwrap();
                        index
                            .save(&mut txn, id, &value(vector.clone()))
                            .await
                            .unwrap();
                        match txn.commit().await {
                            Ok(_) => {
                                latencies.push(begun.elapsed());
                                break;
                            }
                            Err(error) if is_conflict(&error) => conflicts += 1,
                            Err(error) => panic!("insert {id} failed: {error}"),
                        }
                    }
                }
            })
            .await
            .unwrap(),
        );
    }
    let (mut conflicts, mut latencies) = (0, Vec::new());
    for task in tasks {
        let (c, l) = task.await.unwrap();
        conflicts += c;
        latencies.extend(l);
    }
    let elapsed = started.elapsed();
    latencies.sort();
    Run {
        commits: latencies.len() as u64,
        conflicts,
        latencies,
        elapsed,
    }
}

fn percentile(sorted: &[Duration], p: usize) -> Duration {
    sorted[(sorted.len() * p / 100).min(sorted.len() - 1)]
}

#[test]
#[ignore = "bench: run on the bench machine with --ignored --nocapture"]
fn diskann_write_cost() {
    workers::block_on(async {
    let dimensions: usize = env("DIMS", "768").parse().unwrap();
    let inserts: u64 = env("INSERTS", "4000").parse().unwrap();
    for writers in env("WRITERS", "1,4,16,64").split(',') {
        let writers: u64 = writers.parse().unwrap();
        let run = run(dimensions, writers, inserts).await;
        println!(
            "WRITE diskann dims={dimensions} writers={writers}: commits/s={:.0} conflicts/commit={:.4} p50={:.2}ms p99={:.2}ms",
            run.commits as f64 / run.elapsed.as_secs_f64(),
            run.conflicts as f64 / run.commits as f64,
            percentile(&run.latencies, 50).as_secs_f64() * 1e3,
            percentile(&run.latencies, 99).as_secs_f64() * 1e3,
        );
    }
    })
    .unwrap();
}
