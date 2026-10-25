//! Concurrent standalone reads through the HTTP router, on an on-disk store
//! placed on the node's worker pool, wired as a server wires them. No
//! network, signing or ACP.
//!
//! `cargo bench -p benches --bench read_baseline --profile release-fast`
//! runs 1, 4, 16 and 64 readers, 1024 requests each, three times, for a read
//! of one document by its ID and a page of ten. Override with
//! DEFRA_BASELINE_READERS (comma-separated), DEFRA_BASELINE_OPERATIONS and
//! DEFRA_BASELINE_REPETITIONS. Raw samples and provenance go to stdout as JSON
//! lines; dashboard families use DEFRA_BENCH_OUT through the existing emitter.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use defra_perf::emit::{Family, Group, Row, Trust};
use defra_perf::measure::{median, min_max, percentile_ms};
use query::mutator::DocMutator;
use query::QueryRequest;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::corekv::Store;
use storage::{RegolithStore, RegolithStoreOptions};
use tower::ServiceExt;

mod common;
use common::{counts, git, positive};

const COLLECTION: &str = "User";
const DOCUMENTS: usize = 1024;
const PAGE: usize = 10;

#[derive(Clone, Copy)]
enum Shape {
    Point,
    Page,
}

impl Shape {
    const ALL: [Self; 2] = [Self::Point, Self::Page];

    fn name(self) -> &'static str {
        match self {
            Self::Point => "point",
            Self::Page => "page",
        }
    }

    fn query(self, reader: usize, operation: usize, ids: &[String]) -> String {
        match self {
            Self::Point => {
                let id = &ids[(reader * 7919 + operation) % ids.len()];
                format!(r#"query {{ User(docID: "{id}") {{ name age }} }}"#)
            }
            Self::Page => format!("query {{ User(limit: {PAGE}) {{ name age }} }}"),
        }
    }

    fn rows(self) -> usize {
        match self {
            Self::Point => 1,
            Self::Page => PAGE,
        }
    }
}

struct Measurement {
    elapsed: Duration,
    latencies: Vec<Duration>,
    pool_tasks: u64,
}

struct Fixture {
    router: Router,
    pool: workers::Pool,
    store: Arc<RegolithStore>,
    ids: Arc<[String]>,
    _directory: tempfile::TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let pool = storage::start_pool(workers::Config::default(), None).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(
            RegolithStore::open_with_options(
                directory.path(),
                RegolithStoreOptions::new().placed_on(&pool),
            )
            .unwrap(),
        );
        let db = Arc::new(
            db::DB::open_from_arc_with_options(
                store.clone(),
                db::DbOptions::new().with_pool(pool.clone()),
            )
            .await
            .unwrap(),
        );
        db.create_collection(CollectionVersion::new(
            COLLECTION,
            "read-baseline-v1",
            "read-baseline",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
                FieldDescription::new("3", "age", FieldKind::int(), schema::CType::LwwRegister),
            ],
        ))
        .await
        .unwrap();
        let collection = db.get_collection(COLLECTION).unwrap().unwrap();
        let runner = query::QueryRunner::with_registry(
            db::LensedAutoCommitFetcher::new(db.clone()),
            vec![collection.schema().clone()],
            db::DbTransactionRegistry::new(db.clone()),
        )
        .with_mutator(
            Arc::new(db::write::autocommit::AutoCommitMutator::new(db.clone()))
                as Arc<dyn DocMutator>,
        );
        let executor = query::pooled::PooledExecutor::wrap(Arc::new(runner), db.pool());
        let mut ids = Vec::with_capacity(DOCUMENTS);
        for index in 0..DOCUMENTS {
            let response = executor
                .execute(QueryRequest::new(format!(
                    r#"mutation {{ add_User(input: {{name: "user-{index}", age: {}}}) {{ _docID }} }}"#,
                    18 + index % 80
                )))
                .await;
            assert!(response.errors.is_empty(), "seed: {:?}", response.errors);
            ids.push(
                response.data.unwrap()["add_User"][0]["_docID"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            );
        }
        let router = defra_http::Server::from_arc(executor, edge::testing::edge())
            .with_pool(Some(pool.clone()))
            .router()
            .unwrap();
        Self {
            router,
            pool,
            store,
            ids: ids.into(),
            _directory: directory,
        }
    }

    fn pool_tasks(&self) -> u64 {
        self.pool
            .stats()
            .workers
            .iter()
            .map(|worker| worker.tasks)
            .sum()
    }
}

fn request(query: &str) -> Request<Body> {
    Request::post("/api/v0/graphql")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "query": query }).to_string(),
        ))
        .unwrap()
}

/// Sends `query` and checks the answer holds `rows` users, returning how long
/// the router took to answer.
async fn read(router: Router, query: &str, rows: usize) -> Duration {
    let start = Instant::now();
    let response = router.oneshot(request(query)).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let elapsed = start.elapsed();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let answer: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(answer.get("errors").is_none(), "{answer}");
    assert_eq!(
        answer["data"][COLLECTION].as_array().map(Vec::len),
        Some(rows),
        "{answer}"
    );
    elapsed
}

async fn run(fixture: &Fixture, shape: Shape, readers: usize, operations: usize) -> Measurement {
    let stats = fixture.store.transaction_stats_handle().unwrap();
    let off_pool = stats.snapshot().off_pool_calls;
    let tasks = fixture.pool_tasks();
    let start = Instant::now();
    let mut set = tokio::task::JoinSet::new();
    for reader in 0..readers {
        let (router, ids) = (fixture.router.clone(), fixture.ids.clone());
        set.spawn(async move {
            let mut latencies = Vec::with_capacity(operations);
            for operation in 0..operations {
                let query = shape.query(reader, operation, &ids);
                latencies.push(read(router.clone(), &query, shape.rows()).await);
            }
            latencies
        });
    }
    let mut latencies = Vec::with_capacity(readers * operations);
    while let Some(sample) = set.join_next().await {
        latencies.extend(sample.unwrap());
    }
    let elapsed = start.elapsed();
    assert_eq!(
        stats.snapshot().off_pool_calls,
        off_pool,
        "every read must run on the pool"
    );
    Measurement {
        elapsed,
        pool_tasks: fixture.pool_tasks() - tasks,
        latencies,
    }
}

fn metrics(sample: &Measurement) -> [(&'static str, &'static str, bool, f64); 4] {
    let reads = sample.latencies.len() as f64;
    [
        (
            "reads",
            "reads/s",
            false,
            reads / sample.elapsed.as_secs_f64(),
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
        (
            "pool tasks",
            "tasks/read",
            true,
            sample.pool_tasks as f64 / reads,
        ),
    ]
}

fn main() {
    let readers = counts("DEFRA_BASELINE_READERS", "1,4,16,64");
    let operations = positive("DEFRA_BASELINE_OPERATIONS", 1024);
    let repetitions = positive("DEFRA_BASELINE_REPETITIONS", 3);
    let guard = common::load_guard();
    println!(
        "{}",
        serde_json::json!({
            "baseline_run": {
                "revision": git(&["rev-parse", "HEAD"]),
                "dirty": !git(&["status", "--porcelain"]).is_empty(),
                "host": defra_perf::run_meta::host(),
                "toolchain": defra_perf::run_meta::toolchain(),
                "profile": std::env::var("DEFRA_BASELINE_PROFILE").unwrap_or_else(|_| "unspecified".into()),
                "store": "disk", "documents": DOCUMENTS, "readers": readers,
                "operations_per_reader": operations, "repetitions": repetitions,
                "load_guard": guard,
            }
        })
    );
    let rt = common::owned_runtime();
    let fixture = rt.block_on(Fixture::new());
    for shape in Shape::ALL {
        rt.block_on(run(&fixture, shape, 1, DOCUMENTS));
        let mut groups: Vec<Group> = Vec::new();
        for &count in &readers {
            let mut values: Vec<Vec<f64>> = Vec::new();
            for repetition in 0..repetitions {
                let sample = rt.block_on(run(&fixture, shape, count, operations));
                let metrics = metrics(&sample);
                if groups.is_empty() {
                    groups = metrics
                        .iter()
                        .map(|(name, unit, lower, _)| {
                            if *lower {
                                Group::lower_better(*name, *unit)
                            } else {
                                Group::higher_better(*name, *unit)
                            }
                            .over("readers")
                        })
                        .collect();
                }
                values.resize_with(metrics.len(), Vec::new);
                println!(
                    "{}",
                    serde_json::json!({
                        "baseline_sample": {
                            "workload": shape.name(), "readers": count, "repetition": repetition,
                            "requests": sample.latencies.len(),
                            "elapsed_seconds": sample.elapsed.as_secs_f64(),
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
            format!("Concurrent reads: {}", shape.name()),
            format!("On-disk store on the worker pool, {DOCUMENTS} documents, warmed; {operations} requests per reader; {repetitions} repetitions. Requests go through the HTTP router as a server builds it, without sockets, signing or ACP. Pool tasks count every task the pool started per read."),
        )
        .trust(if guard.passed { Trust::Clean } else { Trust::Contaminated });
        for group in groups {
            family = family.group(group);
        }
        family.emit(&format!("read_baseline_{}", shape.name()));
    }
    rt.block_on(fixture.store.close()).unwrap();
    fixture.pool.shutdown_blocking().unwrap();
}
