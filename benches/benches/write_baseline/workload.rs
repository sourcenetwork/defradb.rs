use std::sync::Arc;
use std::time::{Duration, Instant};

use db::DB;
use defra_http::{AppState, AppStateBuilder, ExtractIdentity};
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use schema::{
    CType, CollectionVersion, FieldDescription, FieldKind, IndexKind, IndexedFieldDescription,
    OrderedIndexDescription,
};
use storage::corekv::Store;
use storage::{RegolithStore, RegolithStoreOptions};
use telemetry::ConflictMetricsSnapshot;

#[cfg(test)]
#[path = "verification_tests.rs"]
mod verification_tests;
#[path = "verify.rs"]
mod verify;

const COLLECTION: &str = "Sample";
pub const SHARED_DOCUMENTS: usize = 8;
pub const MAX_HTTP_RETRIES: u32 = 16;

#[derive(Clone, Copy, Debug)]
pub enum Scenario {
    Creates,
    SharedUpdates,
    CountersIndexes,
}

impl Scenario {
    pub const ALL: [Self; 3] = [Self::Creates, Self::SharedUpdates, Self::CountersIndexes];

    pub fn name(self) -> &'static str {
        match self {
            Self::Creates => "creates",
            Self::SharedUpdates => "shared_updates",
            Self::CountersIndexes => "counters_indexes",
        }
    }

    fn mutation(self, writer: usize, operation: usize, ids: &[String]) -> String {
        let token = format!("writer-{writer}-operation-{operation}");
        match self {
            Self::Creates => format!(
                r#"mutation {{ add_Sample(input: {{label: "{token}", left: "same", right: "same", count: 0}}) {{ _docID }} }}"#
            ),
            Self::SharedUpdates | Self::CountersIndexes => {
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
                    _ => format!(r#"count: 1, label: "{token}""#),
                };
                format!(
                    r#"mutation {{ update_Sample(docID: "{id}", input: {{{input}}}) {{ _docID }} }}"#
                )
            }
        }
    }
}

pub struct Measurement {
    pub elapsed: Duration,
    pub latencies: Vec<Duration>,
    pub commits: u64,
    pub exhaustions: u64,
    pub storage_conflicts: u64,
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
    async fn new(scenario: Scenario) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(
            RegolithStore::open_with_options(directory.path(), RegolithStoreOptions::new())
                .unwrap(),
        );
        let db = Arc::new(DB::open_from_arc(store.clone()).await.unwrap());
        let collection = CollectionVersion::new(
            COLLECTION,
            "write-baseline-v1",
            "write-baseline",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", "label", FieldKind::string()),
                FieldDescription::new("3", "left", FieldKind::string()),
                FieldDescription::new("4", "right", FieldKind::string()),
                FieldDescription::new("5", "count", FieldKind::int())
                    .with_crdt_type(CType::PnCounter),
            ],
        );
        db.create_collection(collection).await.unwrap();
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
            state: Arc::new(
                AppStateBuilder::new(Arc::new(runner))
                    .with_max_txn_retries(MAX_HTTP_RETRIES)
                    .build(),
            ),
            store,
            db,
            ids,
            _directory: directory,
        }
    }
}

pub async fn run(scenario: Scenario, writers: usize, operations: usize) -> Measurement {
    assert!(writers > 0 && operations > 0);
    let fixture = Fixture::new(scenario).await;
    let stats = fixture.store.transaction_stats_handle().unwrap();
    let storage_before = stats.snapshot();
    let retries_before = telemetry::conflict_metrics_snapshot();
    let start = Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    for writer in 0..writers {
        let state = fixture.state.clone();
        let ids = fixture.ids.clone();
        tasks.spawn(async move {
            let mut latencies = Vec::with_capacity(operations);
            let mut successes = Vec::with_capacity(operations);
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
                if !response.is_transaction_conflict() {
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
            (latencies, successes)
        });
    }
    let mut latencies = Vec::with_capacity(writers * operations);
    let mut successes = Vec::with_capacity(writers * operations);
    while let Some(result) = tasks.join_next().await {
        let (samples, succeeded) = result.unwrap();
        latencies.extend(samples);
        successes.extend(succeeded);
    }
    let elapsed = start.elapsed();
    let commits = successes.len() as u64;
    let measurement = Measurement {
        elapsed,
        latencies,
        commits,
        exhaustions: (writers * operations) as u64 - commits,
        storage_conflicts: stats.snapshot().conflicts - storage_before.conflicts,
        retries_before,
        retries_after: telemetry::conflict_metrics_snapshot(),
    };
    verify::check(&fixture, scenario, &successes).await;
    fixture.store.close().await.unwrap();
    measurement
}
