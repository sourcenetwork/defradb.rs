//! Insert throughput into a trained DiskANN index on an on-disk regolith
//! store, at several concurrent writers: what each pending insert costs, and
//! whether writers conflict. Three shapes of writer:
//!
//! - `index`: writes through `VectorIndex` alone;
//! - `straddle`: the same, while writers whose snapshot predates a rebuild
//!   share their own training's quantizer between every two writes;
//! - `explicit`: document creates through the database, each in its own
//!   explicit transaction.
//!
//! Ignored; run on the bench machine:
//!
//! ```text
//! DIMS=768 INSERTS=4000 WRITERS=1,4,16,64 MODES=index,straddle,explicit TMPDIR=/data/tmp \
//!   cargo test --release -p db --test vector diskann_write_cost -- --ignored --nocapture
//! ```
//!
//! Prints one `WRITE` line per mode and writer count: commits per second,
//! conflicts per commit, and p50 and p99 commit latency.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::support::Corpus;
use db::index::vector::engine::diskann::codec;
use db::index::vector::engine::diskann::TRAIN_THRESHOLD;
use db::index::vector::engine::diskann::{CodeCache, CodeCaches};
use db::index::vector::index::VectorIndex;
use db::index::vector::store::{KvNodeStore, VectorNodeStore};
use db::{DbDocMutator, DB};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use schema::{
    CollectionVersion, DistanceMetric, FieldDescription, FieldKind, IndexDescription,
    IndexedFieldDescription, VectorAlgorithm, VectorIndexDescription,
};
use storage::corekv::{Error, Store};
use storage::index::CollectionIndex;
use storage::RegolithStore;

const COLLECTION: u32 = 41;
const INDEX_ID: u32 = 1;
const COLLECTION_NAME: &str = "Embeddings";

/// The tag the straddling writers' older training drew. A real tag is random,
/// so this one is another training's with overwhelming odds.
const OLDER_TRAINING: u128 = 0x01de_0001;

/// Worker threads the writers are spread over.
const THREADS: usize = 8;

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn description(dimensions: usize) -> IndexDescription {
    IndexDescription {
        name: "by_embedding".to_string(),
        id: INDEX_ID,
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

/// The tag of the training the store's trained record names.
async fn training(store: &RegolithStore, collection: u32) -> u128 {
    let mut read = store.new_txn(true).await.unwrap();
    let bytes = KvNodeStore::new(&mut read, collection, INDEX_ID, 0)
        .get_aux(codec::TRAINED, b"")
        .await
        .unwrap()
        .expect("the index has trained");
    read.discard();
    codec::decode_trained(&bytes)
        .unwrap()
        .training
        .expect("a training draws its tag")
}

/// A store whose index has trained, so every later insert is a pending one,
/// and the cache its writes share their quantizer through.
async fn trained(
    dir: &std::path::Path,
    dimensions: usize,
) -> (RegolithStore, Arc<VectorIndex>, Arc<CodeCache>) {
    let store = RegolithStore::open(dir).unwrap();
    let mut index = VectorIndex::try_new(COLLECTION, description(dimensions)).unwrap();
    let caches = CodeCaches::default();
    index.share_quantizer(&caches);
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
    (store, index, caches.get(COLLECTION, INDEX_ID))
}

#[derive(Default)]
struct Run {
    commits: u64,
    conflicts: u64,
    latencies: Vec<Duration>,
    elapsed: Duration,
}

impl Run {
    fn print(mut self, mode: &str, dimensions: usize, writers: u64) {
        self.latencies.sort();
        println!(
            "WRITE diskann mode={mode} dims={dimensions} writers={writers}: commits/s={:.0} conflicts/commit={:.4} p50={:.2}ms p99={:.2}ms",
            self.commits as f64 / self.elapsed.as_secs_f64(),
            self.conflicts as f64 / self.commits as f64,
            percentile(&self.latencies, 50).as_secs_f64() * 1e3,
            percentile(&self.latencies, 99).as_secs_f64() * 1e3,
        );
    }
}

/// Spreads `writers` over the pool; each takes the next id until `last` and
/// calls `insert`, which answers whether the commit conflicted.
async fn race<C, F, Fut>(writers: u64, first: u64, last: u64, context: C, insert: F) -> Run
where
    C: Clone + Send + 'static,
    F: Fn(C, u64, u64) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = bool>,
{
    let next = Arc::new(AtomicU64::new(first));
    let pool = workers::Pool::start(workers::Config::unplaced(THREADS)).unwrap();
    let started = Instant::now();
    let mut tasks = Vec::with_capacity(writers as usize);
    for writer in 0..writers {
        let (context, next, insert) = (context.clone(), next.clone(), insert.clone());
        let target = workers::Target::Worker(writer as usize % THREADS);
        tasks.push(
            pool.submit(target, move || async move {
                let (mut conflicts, mut latencies) = (0, Vec::new());
                loop {
                    let id = next.fetch_add(1, Ordering::Relaxed);
                    if id > last {
                        return (conflicts, latencies);
                    }
                    loop {
                        let begun = Instant::now();
                        if insert(context.clone(), writer, id).await {
                            conflicts += 1;
                        } else {
                            latencies.push(begun.elapsed());
                            break;
                        }
                    }
                }
            })
            .await
            .unwrap(),
        );
    }
    let mut run = Run::default();
    for task in tasks {
        let (conflicts, latencies) = task.await.unwrap();
        run.conflicts += conflicts;
        run.latencies.extend(latencies);
    }
    run.elapsed = started.elapsed();
    run.commits = run.latencies.len() as u64;
    run
}

/// Inserts through `VectorIndex`. With `straddle`, every other insert is
/// preceded by an older snapshot's writer sharing its training's quantizer,
/// as it does after building one of its own.
async fn index_run(dimensions: usize, writers: u64, inserts: u64, straddle: bool) -> Run {
    let dir = tempfile::tempdir().unwrap();
    let (store, index, cache) = trained(dir.path(), dimensions).await;
    let mut warm = store.new_txn(false).await.unwrap();
    index
        .save(
            &mut warm,
            TRAIN_THRESHOLD + 1,
            &value(Corpus::new(1).vector(dimensions)),
        )
        .await
        .unwrap();
    warm.commit().await.unwrap();
    let older = cache
        .quantizer(training(&store, COLLECTION).await)
        .expect("the warm-up write shared its quantizer");
    let first = TRAIN_THRESHOLD + 2;
    race(
        writers,
        first,
        first + inserts - 1,
        (store, index, cache, older),
        move |(store, index, cache, older), writer, id| async move {
            if straddle && id % 2 == 0 {
                cache.share_quantizer(OLDER_TRAINING, older);
            }
            let vector = Corpus::new(0xA000 + writer * 1_000_003 + id).vector(dimensions);
            let mut txn = store.new_txn(false).await.unwrap();
            index.save(&mut txn, id, &value(vector)).await.unwrap();
            match txn.commit().await {
                Ok(_) => false,
                Err(error) if is_conflict(&error) => true,
                Err(error) => panic!("insert {id} failed: {error}"),
            }
        },
    )
    .await
}

fn collection(dimensions: usize) -> CollectionVersion {
    let mut version = CollectionVersion::new(
        COLLECTION_NAME,
        "bafkdiskannwrite",
        "diskann-write",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "2",
                "embedding",
                FieldKind::float32_array(),
                schema::CType::LwwRegister,
            ),
        ],
    );
    version.indexes = vec![description(dimensions)];
    version
}

fn embedding(version: &Arc<CollectionVersion>, vector: Vec<f32>) -> Document {
    let mut doc = Document::new(Arc::clone(version));
    doc.set("embedding", NormalValue::Float32Array(vector))
        .unwrap();
    doc
}

/// Creates through the database, each in its own explicit transaction.
async fn explicit_run(dimensions: usize, writers: u64, inserts: u64) -> Run {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(DB::new(RegolithStore::open(dir.path()).unwrap()).unwrap());
    db.create_collection(collection(dimensions)).await.unwrap();
    let version = Arc::new(
        db.get_collection(COLLECTION_NAME)
            .unwrap()
            .expect("the collection exists")
            .schema()
            .clone(),
    );
    let mut corpus = Corpus::new(0x7121);
    let train = DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
    for _ in 0..TRAIN_THRESHOLD {
        train
            .create(
                COLLECTION_NAME,
                embedding(&version, corpus.vector(dimensions)),
            )
            .await
            .unwrap();
    }
    train.take_txn().await.unwrap().commit().await.unwrap();
    race(
        writers,
        1,
        inserts,
        (db, version),
        move |(db, version), writer, id| async move {
            let vector = Corpus::new(0xA000 + writer * 1_000_003 + id).vector(dimensions);
            let mut txn = db.new_txn(false).await.unwrap();
            txn.make_explicit();
            let mutator = DbDocMutator::new(db.clone(), txn);
            mutator
                .create(COLLECTION_NAME, embedding(&version, vector))
                .await
                .unwrap();
            match mutator.take_txn().await.unwrap().force_commit().await {
                Ok(()) => false,
                Err(error) if error.is_txn_conflict() => true,
                Err(error) => panic!("create {id} failed: {error}"),
            }
        },
    )
    .await
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
        for mode in env("MODES", "index,straddle,explicit").split(',') {
            for writers in env("WRITERS", "1,4,16,64").split(',') {
                let writers: u64 = writers.parse().unwrap();
                let run = match mode {
                    "index" => index_run(dimensions, writers, inserts, false).await,
                    "straddle" => index_run(dimensions, writers, inserts, true).await,
                    "explicit" => explicit_run(dimensions, writers, inserts).await,
                    other => panic!("unknown mode {other}"),
                };
                run.print(mode, dimensions, writers);
            }
        }
    })
    .unwrap();
}
