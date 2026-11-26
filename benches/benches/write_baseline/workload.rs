use std::sync::Arc;
use std::time::{Duration, Instant};

use db::DB;
use defra_http::{AppState, AppStateBuilder, ExtractIdentity};
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use schema::{IndexKind, IndexedFieldDescription, OrderedIndexDescription};
use storage::corekv::Store;
use storage::{RegolithStore, RegolithStoreOptions};
use telemetry::ConflictMetricsSnapshot;

#[path = "projector.rs"]
mod projector;
#[cfg(test)]
#[path = "verification_tests.rs"]
mod verification_tests;
#[path = "verify.rs"]
mod verify;

const COLLECTION: &str = "Sample";
pub const SHARED_DOCUMENTS: usize = 8;

/// Dimensions of the `vec` field `DEFRA_BASELINE_VECTOR` adds.
const VECTOR_DIMENSIONS: usize = 128;

/// `DEFRA_BASELINE_VECTOR=sync|deferred` adds a `vec` field under an HNSW
/// index with that maintenance mode, and every create writes a vector to it.
pub fn vector_maintenance() -> Option<String> {
    std::env::var("DEFRA_BASELINE_VECTOR").ok().map(|mode| {
        assert!(
            mode == "sync" || mode == "deferred",
            "DEFRA_BASELINE_VECTOR is sync or deferred"
        );
        mode
    })
}

/// A unit vector that differs per write, spread so the graph has real work.
fn vector_literal(writer: usize, operation: usize) -> String {
    let seed = (writer * 7919 + operation * 104_729 + 1) as f64;
    let elements: Vec<String> = (0..VECTOR_DIMENSIONS)
        .map(|i| format!("{:.5}", (seed * (i as f64 + 1.0) * 0.618_034).sin()))
        .collect();
    format!("[{}]", elements.join(", "))
}

/// `DEFRA_BASELINE_CHUNK_MAX_CHARS` makes `left` a `@chunk` field cut at that size.
pub fn chunk_max_chars() -> Option<u32> {
    std::env::var("DEFRA_BASELINE_CHUNK_MAX_CHARS")
        .ok()
        .map(|value| {
            value
                .parse()
                .expect("DEFRA_BASELINE_CHUNK_MAX_CHARS is a number")
        })
}

/// `DEFRA_BASELINE_EMBEDDING_URL` embeds the chunked `left` through that provider.
pub fn embedding_url() -> Option<String> {
    // Only a chunked field is embedded, so the URL alone changes nothing.
    chunk_max_chars()?;
    std::env::var("DEFRA_BASELINE_EMBEDDING_URL").ok()
}

#[derive(Clone, Copy, Debug)]
pub enum Scenario {
    Creates,
    SharedUpdates,
    CountersIndexes,
    /// Increments of a counter with no index, the cost a counter index adds to.
    Counters,
    /// Increments of a counter an index covers.
    CounterIndex,
}

impl Scenario {
    pub const ALL: [Self; 5] = [
        Self::Creates,
        Self::SharedUpdates,
        Self::CountersIndexes,
        Self::Counters,
        Self::CounterIndex,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Creates => "creates",
            Self::SharedUpdates => "shared_updates",
            Self::CountersIndexes => "counters_indexes",
            Self::Counters => "counters",
            Self::CounterIndex => "counter_index",
        }
    }

    fn mutation(self, writer: usize, operation: usize, ids: &[String]) -> String {
        let token = format!("writer-{writer}-operation-{operation}");
        match self {
            Self::Creates => {
                let vector = vector_maintenance()
                    .map(|_| format!(", vec: {}", vector_literal(writer, operation)))
                    .unwrap_or_default();
                // Each create embeds a text of its own, so the cache answers none of them.
                let left = if embedding_url().is_some() {
                    token.as_str()
                } else {
                    "same"
                };
                format!(
                    r#"mutation {{ add_Sample(input: {{label: "{token}", left: "{left}", right: "same", count: 0{vector}}}) {{ _docID }} }}"#
                )
            }
            Self::SharedUpdates | Self::CountersIndexes | Self::Counters | Self::CounterIndex => {
                let id = &ids[operation % ids.len()];
                let input = match self {
                    Self::SharedUpdates => {
                        let field = if writer.is_multiple_of(2) {
                            "left"
                        } else {
                            "right"
                        };
                        format!(r#"{field}: "{token}""#)
                    }
                    Self::Counters | Self::CounterIndex => "count: 1".to_string(),
                    _ => format!(r#"count: 1, label: "{token}""#),
                };
                format!(
                    r#"mutation {{ update_Sample(docID: "{id}", input: {{{input}}}) {{ _docID }} }}"#
                )
            }
        }
    }
}

#[allow(dead_code)] // Some fields are consumed only by the benchmark target.
pub struct Measurement {
    pub elapsed: Duration,
    pub latencies: Vec<Duration>,
    pub commits: u64,
    pub client_conflicts: u64,
    pub engine_exhaustions: u64,
    pub storage_conflicts: u64,
    pub engine_retries: u64,
    pub retries_before: ConflictMetricsSnapshot,
    pub retries_after: ConflictMetricsSnapshot,
}

struct Fixture {
    state: Arc<AppState>,
    store: Arc<RegolithStore>,
    db: Arc<DB<RegolithStore>>,
    ids: Vec<String>,
    _directory: tempfile::TempDir,
}

impl Fixture {
    async fn new(scenario: Scenario, edge: &edge::Edge) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(
            RegolithStore::open_with_options(directory.path(), RegolithStoreOptions::new())
                .unwrap(),
        );
        let db = Arc::new(DB::open_from_arc(store.clone()).await.unwrap());
        if projector::requested() {
            projector::install(&db);
        }
        let chunk = chunk_max_chars()
            .map(|max_chars| format!("@chunk(maxChars: {max_chars})"))
            .unwrap_or_default();
        let vector = vector_maintenance()
            .map(|mode| {
                let maintenance = match mode.as_str() {
                    "deferred" => "maintenance: DEFERRED, ",
                    _ => "",
                };
                format!(
                    "vec: [Float32!] @index(vector: {{dimensions: {VECTOR_DIMENSIONS}, {maintenance}hnsw: {{metric: COSINE}}}})"
                )
            })
            .unwrap_or_default();
        let embedded = embedding_url()
            .map(|url| {
                format!(
                    r#"left_v: [Float32!] @embedding(fields: ["left"], provider: "openai", model: "bench", url: "{url}")"#
                )
            })
            .unwrap_or_default();
        let sdl = format!(
            r#"type {COLLECTION} {{
                label: String
                left: String {chunk}
                {embedded}
                right: String
                count: Int @crdt(type: "pncounter")
                {vector}
            }}"#
        );
        db.add_collection(&defra_core::RequestContext::ambient(), &sdl)
            .await
            .unwrap();
        if matches!(scenario, Scenario::CountersIndexes) {
            db.create_index(
                COLLECTION,
                Some("by_label"),
                vec![IndexedFieldDescription {
                    name: "label".into(),
                    descending: false,
                }],
                IndexKind::Ordered(OrderedIndexDescription { unique: false }),
            )
            .await
            .unwrap();
        }
        if matches!(scenario, Scenario::CounterIndex) {
            db.create_index(
                COLLECTION,
                Some("by_count"),
                vec![IndexedFieldDescription {
                    name: "count".into(),
                    descending: false,
                }],
                IndexKind::Ordered(OrderedIndexDescription { unique: false }),
            )
            .await
            .unwrap();
        }
        let collection = db.get_collection(COLLECTION).unwrap().unwrap();
        let runner = QueryRunner::new(
            db::LensedAutoCommitFetcher::new(db.clone()),
            vec![collection.schema().clone()],
        )
        .with_mutator(
            Arc::new(db::write::autocommit::AutoCommitMutator::new(db.clone()))
                as Arc<dyn DocMutator>,
        );
        let mut ids = Vec::new();
        if !matches!(scenario, Scenario::Creates) {
            for index in 0..SHARED_DOCUMENTS {
                let response = runner.execute(QueryRequest::new(format!(
                    r#"mutation {{ add_Sample(input: {{label: "seed-{index}", left: "initial", right: "initial", count: 0}}) {{ _docID }} }}"#
                ))).await;
                assert!(response.errors.is_empty(), "seed: {:?}", response.errors);
                ids.push(
                    response.data.unwrap()["add_Sample"][0]["_docID"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                );
            }
        }
        Self {
            state: Arc::new(AppStateBuilder::new(Arc::new(runner), edge.clone()).build()),
            store,
            db,
            ids,
            _directory: directory,
        }
    }
}

pub async fn run(
    edge: &edge::Edge,
    scenario: Scenario,
    writers: usize,
    operations: usize,
) -> Measurement {
    assert!(writers > 0 && operations > 0);
    let fixture = Fixture::new(scenario, edge).await;
    // A node drains deferred indexes on a database worker as their commits
    // ask; this harness has none, so a loop on the edge stands in for it.
    let draining = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let drainer = vector_maintenance()
        .filter(|mode| mode == "deferred")
        .map(|_| {
            let (db, edge_handle, draining) = (fixture.db.clone(), edge.clone(), draining.clone());
            edge.spawn(async move {
                while draining.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = db.drain_deferred_indexes().await;
                    edge_handle.sleep(Duration::from_millis(1)).await;
                }
            })
        });
    let stats = fixture.store.transaction_stats_handle().unwrap();
    let storage_before = stats.snapshot();
    let retries_before = telemetry::conflict_metrics_snapshot();
    let start = Instant::now();
    let tasks: Vec<_> = (0..writers)
        .map(|writer| {
            let state = fixture.state.clone();
            let ids = fixture.ids.clone();
            edge.spawn(async move {
                let mut latencies = Vec::with_capacity(operations);
                let mut successes = Vec::with_capacity(operations);
                let mut engine_exhaustions = 0u64;
                let mut client_conflicts = 0u64;
                for operation in 0..operations {
                    let request = QueryRequest::new(scenario.mutation(writer, operation, &ids));
                    let start = Instant::now();
                    let response = defra_http::query_context::execute_with_context(
                        &state,
                        &ExtractIdentity::anonymous(),
                        request,
                    )
                    .await;
                    latencies.push(start.elapsed());
                    if response.is_transaction_retry_exhausted() {
                        engine_exhaustions += 1;
                        continue;
                    }
                    if response.is_transaction_conflict() {
                        client_conflicts += 1;
                        continue;
                    }
                    {
                        assert!(
                            response.errors.is_empty(),
                            "{}: {:?}",
                            scenario.name(),
                            response.errors
                        );
                        let data = response.data.unwrap();
                        let key = if matches!(scenario, Scenario::Creates) {
                            "add_Sample"
                        } else {
                            "update_Sample"
                        };
                        assert_eq!(
                            data[key]
                                .as_array()
                                .unwrap_or_else(|| panic!("expected {key} array in {data}"))
                                .len(),
                            1,
                            "exactly one document written"
                        );
                        if !matches!(scenario, Scenario::Creates) {
                            assert_eq!(data[key][0]["_docID"], ids[operation % ids.len()]);
                        }
                        successes.push((writer, operation));
                    }
                }
                (latencies, successes, client_conflicts, engine_exhaustions)
            })
        })
        .collect();
    let mut latencies = Vec::with_capacity(writers * operations);
    let mut successes = Vec::with_capacity(writers * operations);
    let mut client_conflicts = 0u64;
    let mut engine_exhaustions = 0u64;
    for task in tasks {
        let (samples, succeeded, http_failed, engine_failed) = task.await.unwrap();
        latencies.extend(samples);
        successes.extend(succeeded);
        client_conflicts += http_failed;
        engine_exhaustions += engine_failed;
    }
    let elapsed = start.elapsed();
    draining.store(false, std::sync::atomic::Ordering::Relaxed);
    if let Some(drainer) = drainer {
        drainer.await.unwrap();
    }
    let commits = successes.len() as u64;
    let retries_after = telemetry::conflict_metrics_snapshot();
    let measurement = Measurement {
        elapsed,
        latencies,
        commits,
        client_conflicts,
        engine_exhaustions,
        storage_conflicts: stats.snapshot().conflicts - storage_before.conflicts,
        engine_retries: retries_after.engine.attempts - retries_before.engine.attempts,
        retries_before,
        retries_after,
    };
    verify::check(&fixture, scenario, &successes).await;
    fixture.store.close().await.unwrap();
    measurement
}
