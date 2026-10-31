//! Concurrent merges of replicated documents and concurrent filtered
//! truncates, on a fresh on-disk store at Immediate durability for every
//! sample: the two paths the Shinzo core data features (#2439) touch.
//!
//! `cargo bench -p benches --bench shinzo_baseline --profile release-fast`
//! runs 1 and 16 writers, 128 documents each, three times. Override with
//! DEFRA_BASELINE_WRITERS (comma-separated), DEFRA_BASELINE_OPERATIONS and
//! DEFRA_BASELINE_REPETITIONS; DEFRA_BASELINE_REVISION names the tree measured.
//! Each sample is a JSON line on stdout.
//!
//! - `merges`: each writer merges its own new documents, whose blocks are
//!   already in the blockstore, through `DbMergeHandler::handle_block`.
//! - `truncates`: one filtered truncate removes every writer's documents
//!   (a collection runs one truncate at a time), deleting their history and
//!   checking every block's owners; `writers` scales its size.

use std::sync::Arc;
use std::time::{Duration, Instant};

use blockstore::DefraBlockstore;
use db::merge::DbMergeHandler;
use db::DB;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use defra_perf::emit::{Family, Group, Row, Trust};
use defra_perf::measure::{median, min_max, percentile_ms};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::corekv::Store;
use storage::{RegolithStore, RegolithStoreOptions};

mod common;
use common::{counts, positive};

const COLLECTION: &str = "Sample";
const COLLECTION_ID: &str = "shinzo-baseline";

fn collection() -> CollectionVersion {
    CollectionVersion::new(
        COLLECTION,
        "shinzo-baseline-v1",
        COLLECTION_ID,
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new(
                "2",
                "label",
                FieldKind::string(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("3", "writer", FieldKind::int(), schema::CType::LwwRegister),
            FieldDescription::new(
                "4",
                "shared",
                FieldKind::string(),
                schema::CType::LwwRegister,
            ),
        ],
    )
}

fn document(writer: usize, index: usize) -> Document {
    let mut doc = Document::new(collection());
    doc.set("label", NormalValue::String(format!("w{writer}-{index}")))
        .unwrap();
    doc.set("writer", NormalValue::Int(writer as i64)).unwrap();
    // One value every document shares, so every merge and every truncate
    // meets a field block with many owners.
    doc.set("shared", NormalValue::String("shared".into()))
        .unwrap();
    doc
}

struct Sample {
    elapsed: Duration,
    latencies: Vec<Duration>,
    commits: u64,
    storage_conflicts: u64,
    engine_retries: u64,
}

struct Fixture {
    store: Arc<RegolithStore>,
    db: Arc<DB<RegolithStore>>,
    _directory: tempfile::TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(
            RegolithStore::open_with_options(directory.path(), RegolithStoreOptions::new())
                .unwrap(),
        );
        let db = Arc::new(DB::open_from_arc(store.clone()).await.unwrap());
        db.create_collection(collection()).await.unwrap();
        Self {
            store,
            db,
            _directory: directory,
        }
    }
}

async fn merges(writers: usize, operations: usize) -> Sample {
    let fixture = Fixture::new().await;
    let blockstore = Arc::new(DefraBlockstore::new(fixture.store.clone(), false));
    let mut work = Vec::with_capacity(writers);
    for writer in 0..writers {
        let mut blocks = Vec::with_capacity(operations);
        for index in 0..operations {
            let built = db::block::builder::build_blocks_from_document(
                &storage::Crdts::default(),
                &document(writer, index),
                &collection(),
                &blockstore,
            )
            .await
            .unwrap();
            blocks.push(built);
        }
        work.push(blocks);
    }
    let handler = Arc::new(DbMergeHandler::new(fixture.db.clone(), blockstore));
    measure(&fixture, writers, |writer| {
        let handler = handler.clone();
        let blocks = std::mem::take(&mut work[writer]);
        async move {
            let mut latencies = Vec::with_capacity(blocks.len());
            for built in blocks {
                let metadata = BlockMetadata::normal(
                    &built.doc_id,
                    COLLECTION_ID,
                    "did:key:z6MkrShinzoBaseline",
                    Some("peer"),
                    false,
                );
                let start = Instant::now();
                let outcome = handler
                    .handle_block(&built.cid, &built.block, metadata)
                    .await
                    .unwrap();
                latencies.push(start.elapsed());
                assert_eq!(outcome, MergeOutcome::Merged);
            }
            latencies
        }
    })
    .await
}

async fn truncates(writers: usize, operations: usize) -> Sample {
    let fixture = Fixture::new().await;
    let mutator = db::AutoCommitMutator::new(fixture.db.clone());
    for writer in 0..writers {
        for index in 0..operations {
            mutator
                .create(COLLECTION, document(writer, index))
                .await
                .unwrap();
        }
    }
    // A collection runs one truncate at a time (`ActionInProgress`), so the
    // writers' documents go in one truncate: `writers` scales its size.
    let mut sample = measure(&fixture, 1, |_| {
        let db = fixture.db.clone();
        async move {
            let mut conditions = serde_json::Map::new();
            conditions.insert("writer".into(), serde_json::json!({ "_lt": writers }));
            let start = Instant::now();
            db.truncate_collection_as(
                &defra_core::RequestContext::ambient(),
                COLLECTION,
                Some(&query::documents::Filter::Conditions(conditions)),
            )
            .await
            .unwrap();
            vec![start.elapsed()]
        }
    })
    .await;
    // The truncate removes every document; count them, so the rates
    // read per document as the merges do.
    sample.commits = (writers * operations) as u64;
    sample
}

async fn measure<W, F>(fixture: &Fixture, writers: usize, mut writer: W) -> Sample
where
    W: FnMut(usize) -> F,
    F: std::future::Future<Output = Vec<Duration>> + Send + 'static,
{
    let stats = fixture.store.transaction_stats_handle().unwrap();
    let storage_before = stats.snapshot();
    let retries_before = telemetry::conflict_metrics_snapshot();
    let start = Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..writers {
        tasks.spawn(writer(index));
    }
    let mut latencies = Vec::new();
    while let Some(result) = tasks.join_next().await {
        latencies.extend(result.unwrap());
    }
    let elapsed = start.elapsed();
    let retries_after = telemetry::conflict_metrics_snapshot();
    Sample {
        elapsed,
        commits: latencies.len() as u64,
        latencies,
        storage_conflicts: stats.snapshot().conflicts - storage_before.conflicts,
        engine_retries: retries_after.engine.attempts - retries_before.engine.attempts,
    }
}

/// One sample's numbers: name, unit, whether lower is better, value.
fn metrics(sample: &Sample) -> [(&'static str, &'static str, bool, f64); 5] {
    let commits = sample.commits as f64;
    [
        (
            "commits",
            "commits/s",
            false,
            commits / sample.elapsed.as_secs_f64(),
        ),
        (
            "storage conflicts",
            "conflicts/commit",
            true,
            sample.storage_conflicts as f64 / commits,
        ),
        (
            "engine retries",
            "retries/commit",
            true,
            sample.engine_retries as f64 / commits,
        ),
        (
            "latency p50",
            "ms",
            true,
            percentile_ms(&sample.latencies, 50),
        ),
        (
            "latency p99",
            "ms",
            true,
            percentile_ms(&sample.latencies, 99),
        ),
    ]
}

fn main() {
    let writers = counts("DEFRA_BASELINE_WRITERS", "1,16");
    let operations = positive("DEFRA_BASELINE_OPERATIONS", 128);
    let repetitions = positive("DEFRA_BASELINE_REPETITIONS", 3);
    let guard = common::load_guard();
    println!(
        "{}",
        serde_json::json!({
            "baseline_run": {
                "revision": std::env::var("DEFRA_BASELINE_REVISION").unwrap_or_default(),
                "writers": writers, "operations_per_writer": operations,
                "repetitions": repetitions, "durability": "Immediate", "store": "disk",
                "load_guard": guard,
            }
        })
    );
    let rt = common::owned_runtime();
    for workload in ["merges", "truncates"] {
        let mut groups: Vec<Group> = Vec::new();
        for &count in &writers {
            let mut values: Vec<Vec<f64>> = Vec::new();
            for repetition in 0..repetitions {
                let sample = rt.block_on(async {
                    match workload {
                        "merges" => merges(count, operations).await,
                        _ => truncates(count, operations).await,
                    }
                });
                let metrics = metrics(&sample);
                if groups.is_empty() {
                    groups = metrics
                        .iter()
                        .map(|(name, unit, lower, _)| {
                            match lower {
                                true => Group::lower_better(*name, *unit),
                                false => Group::higher_better(*name, *unit),
                            }
                            .over("writers")
                        })
                        .collect();
                }
                values.resize_with(metrics.len(), Vec::new);
                println!(
                    "{}",
                    serde_json::json!({
                        "baseline_sample": {
                            "workload": workload, "writers": count, "repetition": repetition,
                            "commits": sample.commits,
                            "metrics": metrics.iter().map(|(name, _, _, value)| (*name, *value)).collect::<std::collections::BTreeMap<_, _>>(),
                        }
                    })
                );
                for (samples, (_, _, _, value)) in values.iter_mut().zip(metrics) {
                    samples.push(value);
                }
            }
            for (group, mut samples) in groups.iter_mut().zip(values) {
                let (lo, hi) = min_max(&samples);
                group.rows.push(
                    Row::new(count.to_string(), median(&mut samples))
                        .range(lo, hi)
                        .at(count as f64),
                );
            }
        }
        let mut family = Family::new(
            format!("Shinzo core data paths: {workload}"),
            format!("On-disk Immediate durability; {operations} documents per writer; {repetitions} fresh-store repetitions. merges: each writer merges its own new documents through DbMergeHandler::handle_block. truncates: one filtered truncate removes every writer's documents (a collection runs one truncate at a time), so writers scales its size."),
        )
        .trust(if guard.passed { Trust::Clean } else { Trust::Contaminated });
        for group in groups {
            family = family.group(group);
        }
        family.emit(&format!("shinzo_baseline_{workload}"));
    }
}
