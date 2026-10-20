//! A write that lost a race on a field it writes runs again on top of the
//! winner and merges its write there: it never loses that race twice
//! (`query::update_parts::Run`). A field an index covers stays decided on
//! every run, so a re-run that meets another write of it loses again, and the
//! index keeps exactly one entry for the document.
//!
//! Each race pauses one GraphQL update after its first attempt has read and
//! written, commits a winner, lets the paused attempt lose, pauses its re-run
//! the same way, commits a third update of the field meanwhile, then lets the
//! re-run go on.
//!
//! An update whose heads were read before its transaction builds on the heads
//! its transaction reads when another write landed in between, so its write
//! sits above that one and its index entries name the value the document
//! keeps.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use db::{AutoCommitMutator, LensedAutoCommitFetcher, DB};
use document::NormalValue;
use query::mutator::DocMutator;
use query::planner::index_selection::{IndexScanParams, IndexScanType};
use query::runner::DocFetcher;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use regolith::sync::{Barrier, Event, SegQueue};
use schema::{
    CollectionVersion, FieldDescription, FieldKind, IndexKind, IndexedFieldDescription,
    OrderedIndexDescription,
};
use storage::corekv::{
    Result, RetryAttempt, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue,
    RetryOptions, RetryTxn, Store, TransactionConflict, Txn,
};
use storage::{RegolithStore, RegolithStoreOptions};

/// One stop of the watched write: it says it stopped, then waits to go on.
#[derive(Default)]
struct Stop {
    reached: Event,
    resume: Event,
}

/// Where the watched write and the test meet.
#[derive(Default)]
struct Gate {
    armed: AtomicBool,
    /// Whether the watched write stops once, at the first stop, before its
    /// transaction begins, instead of after each attempt.
    ahead: AtomicBool,
    /// The stops of the watched write's first and second attempts, each
    /// after the attempt has read and written and before it commits.
    stops: [Stop; 2],
    /// The reason each attempt of the watched write was handed: none for the
    /// first, the race it lost for each re-run.
    reasons: SegQueue<Option<TransactionConflict>>,
}

/// A store whose next engine-owned write is watched by its gate.
struct PausingStore {
    inner: Arc<RegolithStore>,
    gate: Arc<Gate>,
}

impl storage::corekv::private::Sealed for PausingStore {}

#[async_trait]
impl Store for PausingStore {
    async fn allocate(&self, key: &[u8], count: u64) -> Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: RetryOptions,
        attempt: RetryAttempt,
    ) -> std::result::Result<RetryAttemptValue, RetryAttemptError> {
        if !self.gate.armed.swap(false, Ordering::SeqCst) {
            return self.inner.transact_retry(options, attempt).await;
        }
        if self.gate.ahead.load(Ordering::SeqCst) {
            let stop = &self.gate.stops[0];
            stop.reached.set();
            stop.resume.wait().await;
            return self.inner.transact_retry(options, attempt).await;
        }
        let attempt = Box::new(WatchedAttempt {
            inner: attempt,
            gate: Arc::clone(&self.gate),
            runs: 0,
        });
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}

struct WatchedAttempt {
    inner: RetryAttempt,
    gate: Arc<Gate>,
    runs: usize,
}

impl RetryAttemptFn for WatchedAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            self.gate.reasons.push(txn.previous_conflict());
            let output = self.inner.run(txn).await?;
            if let Some(stop) = self.gate.stops.get(self.runs) {
                stop.reached.set();
                stop.resume.wait().await;
            }
            self.runs += 1;
            Ok(output)
        })
    }
}

struct Node {
    store: Arc<RegolithStore>,
    db: Arc<DB<PausingStore>>,
    queries: Arc<dyn QueryExecutor>,
    gate: Arc<Gate>,
    id: String,
    _dir: tempfile::TempDir,
}

/// A store on disk at Immediate durability, as the write baseline opens it,
/// holding one `Sample` document whose `left` is `initial`; with `indexed`,
/// `left` is indexed.
async fn node(indexed: bool) -> Node {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(
        RegolithStore::open_with_options(dir.path(), RegolithStoreOptions::new()).unwrap(),
    );
    let gate = Arc::new(Gate::default());
    let db = Arc::new(
        DB::new(PausingStore {
            inner: Arc::clone(&store),
            gate: Arc::clone(&gate),
        })
        .unwrap(),
    );
    db.create_collection(CollectionVersion::new(
        "Sample",
        "sample-v1",
        "col-sample",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "left", FieldKind::string()),
        ],
    ))
    .await
    .unwrap();
    if indexed {
        db.create_index(
            "Sample",
            Some("by_left"),
            vec![IndexedFieldDescription {
                name: "left".into(),
                descending: false,
            }],
            IndexKind::Ordered(OrderedIndexDescription { unique: false }),
        )
        .await
        .unwrap();
    }
    let collection = db.get_collection("Sample").unwrap().unwrap();
    let queries: Arc<dyn QueryExecutor> = Arc::new(
        QueryRunner::new(
            LensedAutoCommitFetcher::new(Arc::clone(&db)),
            vec![collection.schema().clone()],
        )
        .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&db))) as Arc<dyn DocMutator>),
    );
    let created = queries
        .execute(QueryRequest::new(
            r#"mutation { add_Sample(input: {left: "initial"}) { _docID } }"#,
        ))
        .await;
    assert!(created.errors.is_empty(), "{:?}", created.errors);
    let id = created.data.unwrap()["add_Sample"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_owned();
    Node {
        store,
        db,
        queries,
        gate,
        id,
        _dir: dir,
    }
}

/// Set the document's `left` to `value` in an autocommit GraphQL update.
async fn set_left(queries: &dyn QueryExecutor, id: &str, value: &str) {
    let response = queries
        .execute(QueryRequest::new(format!(
            r#"mutation {{ update_Sample(docID: "{id}", input: {{left: "{value}"}}) {{ _docID }} }}"#
        )))
        .await;
    assert!(response.errors.is_empty(), "{value}: {:?}", response.errors);
}

impl Node {
    /// Run the watched update to `paused`; while its first attempt waits,
    /// commit `winner`, and while its second waits, commit `third`.
    async fn race(&self, paused: &str, winner: &str, third: &str) {
        self.gate.armed.store(true, Ordering::SeqCst);
        let watched = {
            let (queries, id, value) = (
                Arc::clone(&self.queries),
                self.id.clone(),
                paused.to_owned(),
            );
            workers::spawn(
                Some(crate::common::worker_io::pool()),
                "test",
                move || async move { set_left(&*queries, &id, &value).await },
            )
            .unwrap()
        };
        for (stop, value) in self.gate.stops.iter().zip([winner, third]) {
            stop.reached.wait().await;
            set_left(&*self.queries, &self.id, value).await;
            stop.resume.set();
        }
        watched.await.unwrap();
    }

    /// The reasons the watched update's attempts were handed, in order.
    fn reasons(&self) -> Vec<Option<regolith::Access>> {
        std::iter::from_fn(|| self.gate.reasons.pop())
            .map(|reason| reason.map(|conflict| conflict.access))
            .collect()
    }

    async fn left(&self) -> String {
        let response = self
            .queries
            .execute(QueryRequest::new("query { Sample { left } }"))
            .await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        response.data.unwrap()["Sample"][0]["left"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// The documents the `by_left` index holds an entry for under `values`,
    /// and how many entries the scan met (every entry when `values` is
    /// empty).
    async fn indexed(&self, values: &[&str]) -> (Vec<String>, u64) {
        let scan_type = if values.is_empty() {
            IndexScanType::PrefixScan {
                prefix_values: Vec::new(),
                reverse: false,
            }
        } else {
            IndexScanType::ExactMatch {
                values: values
                    .iter()
                    .map(|value| NormalValue::String((*value).to_owned()))
                    .collect(),
            }
        };
        let result = LensedAutoCommitFetcher::new(Arc::clone(&self.db))
            .get_by_index_scan(
                "Sample",
                &IndexScanParams {
                    index_name: "by_left".into(),
                    scan_type,
                    limit: None,
                    offset: 0,
                    value_filter: None,
                    cursor_seek: None,
                },
            )
            .await
            .unwrap();
        (result.doc_ids().to_vec(), result.raw_fetches())
    }
}

/// The re-run of an update that lost to a write of its field commits beside
/// a third write of that field instead of losing again. Both sit one priority
/// above the winner, and the field settles them by its last-writer-wins
/// order: at one priority the greater value wins, as every replica settles
/// the two blocks.
#[test]
fn a_rerun_merges_its_write_beside_another_write_of_the_field() {
    workers::block_on(async {
        let node = node(false).await;
        node.race("rerun", "first", "third").await;

        assert_eq!(
            node.reasons(),
            vec![None, Some(regolith::Access::ReadParts)],
            "the update lost once, to the winner, and its re-run committed"
        );
        assert_eq!(node.left().await, "third");
    })
    .unwrap()
}

/// An indexed field is decided on every run: the old index entry is found
/// from it. The re-run that meets a third write loses again, runs on top of
/// it, and leaves the one index entry its value names.
#[test]
fn a_rerun_still_decides_on_an_indexed_field() {
    workers::block_on(async {
        let node = node(true).await;
        node.race("rerun", "first", "third").await;

        assert_eq!(
            node.reasons(),
            vec![
                None,
                Some(regolith::Access::ReadParts),
                Some(regolith::Access::ReadParts)
            ],
            "the update lost to the winner, then its re-run lost to the third write"
        );
        assert_eq!(node.left().await, "rerun");
        assert_eq!(node.indexed(&[]).await, (vec![node.id.clone()], 1));
        assert_eq!(node.indexed(&["rerun"]).await, (vec![node.id.clone()], 1));
    })
    .unwrap()
}

/// Concurrent updates of one field of one document each lose at most once:
/// every request commits, with no more storage conflicts than requests.
#[test]
fn concurrent_updates_of_one_field_each_lose_at_most_once() {
    workers::block_on(async {
        const WRITERS: usize = 16;
        const UPDATES: usize = 16;
        let node = node(false).await;
        let stats = node.store.transaction_stats_handle().unwrap();
        let before = stats.snapshot().conflicts;
        let start = Arc::new(Barrier::new(WRITERS));
        let writers: Vec<_> = (0..WRITERS)
            .map(|writer| {
                let (queries, id, start) = (
                    Arc::clone(&node.queries),
                    node.id.clone(),
                    Arc::clone(&start),
                );
                workers::spawn(
                    Some(crate::common::worker_io::pool()),
                    "test",
                    move || async move {
                        start.wait().await;
                        for update in 0..UPDATES {
                            set_left(
                                &*queries,
                                &id,
                                &format!("writer-{writer:02}-update-{update:02}"),
                            )
                            .await;
                        }
                    },
                )
                .unwrap()
            })
            .collect();
        for writer in writers {
            writer.await.unwrap();
        }

        let conflicts = stats.snapshot().conflicts - before;
        assert!(
            conflicts <= (WRITERS * UPDATES) as u64,
            "{conflicts} conflicts for {} updates",
            WRITERS * UPDATES
        );
        assert!(node.left().await.ends_with("-update-15"));
    })
    .unwrap()
}

/// An update reads the heads it builds on before its transaction. When
/// another write of its field lands in between, its transaction sees that
/// write, so it builds on the heads it reads there: its write sits above the
/// other, the document keeps its value, and the index's one entry names it.
/// Built on the earlier heads, its write would tie the other's priority and
/// lose to the greater value while its index entry replaced the other's.
#[test]
fn an_update_builds_on_heads_that_landed_before_its_transaction() {
    workers::block_on(async {
        let node = node(true).await;
        node.gate.ahead.store(true, Ordering::SeqCst);
        node.gate.armed.store(true, Ordering::SeqCst);
        let watched = {
            let (db, id) = (Arc::clone(&node.db), node.id.clone());
            workers::spawn(
                Some(crate::common::worker_io::pool()),
                "test",
                move || async move {
                    let mut doc = document::Document::new();
                    doc.set_id(document::DocID::from_string(&id).unwrap());
                    doc.set("left".to_string(), NormalValue::String("aaa".to_string()));
                    AutoCommitMutator::new(db)
                        .update("Sample", doc, ["left".to_string()].into_iter().collect())
                        .await
                        .unwrap();
                },
            )
            .unwrap()
        };
        node.gate.stops[0].reached.wait().await;
        set_left(&*node.queries, &node.id, "zzz").await;
        node.gate.stops[0].resume.set();
        watched.await.unwrap();

        assert_eq!(
            node.left().await,
            "aaa",
            "the later update sits above the other"
        );
        assert_eq!(node.indexed(&[]).await, (vec![node.id.clone()], 1));
        assert_eq!(node.indexed(&["aaa"]).await, (vec![node.id.clone()], 1));
    })
    .unwrap()
}
