//! Conflicts are business logic keyed on their reason. A write that loses a
//! race is run again by the engine with the reason it lost, and the re-run
//! decides from what the winner left:
//! - a delete that lost to another delete of the document is a no-op;
//! - an update that lost to a delete reports the document not found;
//! - an update of a field another update changed writes again on top of it;
//! - a create that lost a unique-index race reports the violation;
//! - a compare-and-set that lost reports the conflict with its reason.
//!
//! Each test pauses the first engine attempt of one write after it has read
//! and written, commits a competing write, then lets the first attempt commit
//! and lose.

use async_trait::async_trait;
use db::{AutoCommitMutator, DB};
use document::{Document, NormalValue};
use kovan_queue::seg_queue::SegQueue;
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use regolith::sync::Event;
use schema::{
    CollectionVersion, FieldDescription, FieldKind, IndexDescription, IndexedFieldDescription,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use storage::corekv::{
    Result, RetryAttempt, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue,
    RetryOptions, RetryTxn, Store, TransactionConflict, Txn,
};
use storage::RegolithStore;

/// Where the paused write and the test meet.
#[derive(Default)]
struct Gate {
    armed: AtomicBool,
    paused: Event,
    release: Event,
    /// The reason each attempt of the paused write was handed: none for the
    /// first attempt, the lost race for each re-run.
    reasons: SegQueue<Option<TransactionConflict>>,
}

/// A store whose next engine-owned write pauses after its first attempt has
/// run, until the test releases it.
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
        let watched = self.gate.armed.load(Ordering::SeqCst);
        let attempt = Box::new(PausedAttempt {
            inner: attempt,
            gate: Arc::clone(&self.gate),
            watched,
        });
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}

struct PausedAttempt {
    inner: RetryAttempt,
    gate: Arc<Gate>,
    watched: bool,
}

impl RetryAttemptFn for PausedAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            if self.watched {
                self.gate.reasons.push(txn.previous_conflict());
            }
            let output = self.inner.run(txn).await?;
            if self.watched && self.gate.armed.swap(false, Ordering::SeqCst) {
                self.gate.paused.set();
                self.gate.release.wait().await;
            }
            Ok(output)
        })
    }
}

struct Fixture {
    db: Arc<DB<PausingStore>>,
    gate: Arc<Gate>,
}

impl Fixture {
    async fn new(collection: CollectionVersion) -> Self {
        let gate = Arc::new(Gate::default());
        let store = PausingStore {
            inner: Arc::new(RegolithStore::in_memory().unwrap()),
            gate: Arc::clone(&gate),
        };
        let db = Arc::new(DB::new(store).unwrap());
        db.create_collection(collection).await.unwrap();
        Self { db, gate }
    }

    fn mutator(&self) -> AutoCommitMutator<PausingStore> {
        AutoCommitMutator::new(Arc::clone(&self.db))
    }

    /// Run `paused` until its first attempt has read and written, commit
    /// `winner` meanwhile, then let `paused` commit, lose and run again.
    async fn race<P, PF, PT, W, WF, WT>(&self, paused: P, winner: W) -> (PT, WT)
    where
        P: FnOnce(AutoCommitMutator<PausingStore>) -> PF,
        PF: std::future::Future<Output = PT> + Send + 'static,
        PT: Send + 'static,
        W: FnOnce(AutoCommitMutator<PausingStore>) -> WF,
        WF: std::future::Future<Output = WT>,
    {
        self.gate.armed.store(true, Ordering::SeqCst);
        let paused = workers::spawn(None, "test", move || paused(self.mutator())).unwrap();
        self.gate.paused.wait().await;
        let won = winner(self.mutator()).await;
        self.gate.release.set();
        (paused.await.unwrap(), won)
    }

    /// The reasons the paused write's attempts were handed, in order.
    fn reasons(&self) -> Vec<Option<TransactionConflict>> {
        std::iter::from_fn(|| self.gate.reasons.pop()).collect()
    }
}

fn patch_collection() -> CollectionVersion {
    CollectionVersion::new(
        "Patch",
        "v1",
        "col-patch",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "left", FieldKind::int()),
            FieldDescription::new("3", "right", FieldKind::int()),
        ],
    )
}

fn fields(names: &[&str]) -> RapidHashSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

async fn create(fixture: &Fixture, json: &str) -> Document {
    let created = fixture
        .mutator()
        .create("Patch", Document::from_json_str(json).unwrap())
        .await
        .unwrap();
    fixture
        .mutator()
        .get_for_update("Patch", &created.doc_id)
        .await
        .unwrap()
        .unwrap()
}

/// The paused write ran twice: first with no reason, then with the race it
/// lost.
fn lost_once(reasons: &[Option<TransactionConflict>]) -> TransactionConflict {
    assert_eq!(reasons.len(), 2, "{reasons:?}");
    assert!(reasons[0].is_none());
    reasons[1].expect("the re-run is handed the reason it lost")
}

#[test]
fn a_delete_that_loses_to_a_delete_is_a_no_op() {
    workers::block_on(async {
        let fixture = Fixture::new(patch_collection()).await;
        let doc = create(&fixture, r#"{"left": 1, "right": 1}"#).await;
        let id = doc.id().unwrap().clone();

        let paused_id = id.clone();
        let (loser, winner) = fixture
            .race(
                |mutator| async move { mutator.delete("Patch", &paused_id).await },
                |mutator| async move { mutator.delete("Patch", &id).await },
            )
            .await;

        assert!(winner.unwrap().existed);
        assert!(!loser.unwrap().existed, "the document was already deleted");
        lost_once(&fixture.reasons());
    })
    .unwrap()
}

#[test]
fn an_update_that_loses_to_a_delete_reports_not_found() {
    workers::block_on(async {
        let fixture = Fixture::new(patch_collection()).await;
        let doc = create(&fixture, r#"{"left": 1, "right": 1}"#).await;
        let id = doc.id().unwrap().clone();

        let mut patched = doc.clone();
        patched.set("left", NormalValue::Int(2));
        let (loser, winner) = fixture
            .race(
                |mutator| async move { mutator.update("Patch", patched, fields(&["left"])).await },
                |mutator| async move { mutator.delete("Patch", &id).await },
            )
            .await;

        assert!(winner.unwrap().existed);
        let error = loser.expect_err("the document was deleted before the update committed");
        assert!(
            matches!(error, query::error::QueryError::DocumentNotFound(_)),
            "{error:?}"
        );
        lost_once(&fixture.reasons());
    })
    .unwrap()
}

#[test]
fn an_update_of_a_field_another_update_changed_writes_again_on_top_of_it() {
    workers::block_on(async {
        let fixture = Fixture::new(patch_collection()).await;
        let doc = create(&fixture, r#"{"left": 0, "right": 0}"#).await;
        let id = doc.id().unwrap().clone();

        let mut first = doc.clone();
        first.set("left", NormalValue::Int(1));
        let mut second = doc.clone();
        second.set("left", NormalValue::Int(2));
        let (loser, winner) = fixture
            .race(
                |mutator| async move { mutator.update("Patch", first, fields(&["left"])).await },
                |mutator| async move { mutator.update("Patch", second, fields(&["left"])).await },
            )
            .await;

        winner.unwrap();
        loser.expect("the update that lost runs again on the winner's value and commits");
        let stored = fixture
            .mutator()
            .get_for_update("Patch", &id)
            .await
            .unwrap()
            .unwrap();
        // The re-run committed last, on top of the winner.
        assert_eq!(stored.get("left"), Some(&NormalValue::Int(1)));
        assert_eq!(stored.get("right"), Some(&NormalValue::Int(0)));
        lost_once(&fixture.reasons());
    })
    .unwrap()
}

#[test]
fn a_create_that_loses_a_unique_index_race_reports_the_violation() {
    workers::block_on(async {
        let mut collection = patch_collection();
        collection.indexes = vec![IndexDescription {
            name: "unique_left".to_string(),
            id: 1,
            fields: vec![IndexedFieldDescription {
                name: "left".to_string(),
                descending: false,
            }],
            unique: true,
            kind: None,
            auto_generated: false,
        }];
        let fixture = Fixture::new(collection).await;

        let (loser, winner) = fixture
            .race(
                |mutator| async move {
                    let doc = Document::from_json_str(r#"{"left": 7, "right": 1}"#).unwrap();
                    mutator.create("Patch", doc).await
                },
                |mutator| async move {
                    let doc = Document::from_json_str(r#"{"left": 7, "right": 2}"#).unwrap();
                    mutator.create("Patch", doc).await
                },
            )
            .await;

        winner.unwrap();
        let error = loser.expect_err("the unique value was taken before this create committed");
        assert!(
            error
                .to_string()
                .contains(storage::corekv::UNIQUE_CONSTRAINT_VIOLATION_MESSAGE),
            "{error}"
        );
        lost_once(&fixture.reasons());
    })
    .unwrap()
}

#[test]
fn a_compare_and_set_that_loses_reports_the_conflict_with_its_reason() {
    workers::block_on(async {
        let fixture = Fixture::new(patch_collection()).await;
        let doc = create(&fixture, r#"{"left": 0, "right": 0}"#).await;

        let expected = doc.clone();
        let mut compared = doc.clone();
        compared.set("right", NormalValue::Int(1));
        let mut other = doc.clone();
        other.set("left", NormalValue::Int(1));
        let (loser, winner) = fixture
            .race(
                |mutator| async move {
                    mutator
                        .update_if_unchanged("Patch", expected, compared, fields(&["right"]))
                        .await
                },
                |mutator| async move { mutator.update("Patch", other, fields(&["left"])).await },
            )
            .await;

        winner.unwrap();
        let error = loser.expect_err("the document changed after the compared read");
        let query::error::QueryError::TransactionConflict(message) = error else {
            panic!("a lost compare-and-set is a conflict, got {error:?}");
        };
        assert!(
            message.ends_with("transaction conflict. Please retry"),
            "{message}"
        );
        assert!(message.contains("lost to a newer"), "{message}");
        lost_once(&fixture.reasons());
    })
    .unwrap()
}
