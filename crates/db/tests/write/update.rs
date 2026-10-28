use db::DB;
use db::{AutoCommitMutator, DbDocFetcher, DbDocMutator};
use document::Document;
use document::NormalValue;
use kovan::AtomOption;
use query::mutator::DocMutator;
use query::planner::index_selection::{IndexScanParams, IndexScanType};
use query::runner::DocFetcher;
use rapidhash::RapidHashSet;
use schema::CollectionVersion;
use schema::FieldDescription;
use schema::FieldKind;
use schema::IndexDescription;
use std::sync::Arc;
use storage::RegolithStore;

fn test_collection() -> CollectionVersion {
    CollectionVersion::new(
        "Patch",
        "v1",
        "col-patch",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "left", FieldKind::int(), schema::CType::LwwRegister),
            FieldDescription::new("3", "right", FieldKind::int(), schema::CType::LwwRegister),
        ],
    )
}

/// A query plan materializes the document before entering the auto-commit
/// mutator. Two concurrent plans can therefore hand the mutator separate
/// stale copies of the same document. `modified_fields` is the patch
/// boundary: applying the second copy must not replace an unrelated field
/// written by the first update.
#[test]
fn stale_disjoint_field_update_without_precondition_preserves_committed_fields() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).expect("create db"));
        db.create_collection(
            test_collection()
                .with_index(IndexDescription::new("by_left").with_field("left", false)),
        )
        .await
        .expect("schema");
        let mutator = AutoCommitMutator::new(Arc::clone(&db));

        let initial = Document::from_json_str(r#"{"left": 0, "right": 0}"#, test_collection())
            .expect("initial document");
        let created = mutator.create("Patch", initial).await.expect("create");

        let mut stale_left = mutator
            .get_for_update("Patch", &created.doc_id)
            .await
            .expect("fetch left copy")
            .expect("left copy");
        let mut stale_right = stale_left.clone();

        stale_left.set("left", NormalValue::Int(1)).unwrap();
        mutator
            .update(
                "Patch",
                stale_left,
                RapidHashSet::from_iter(["left".to_string()]),
            )
            .await
            .expect("update left");

        stale_right.set("right", NormalValue::Int(1)).unwrap();
        mutator
            .update(
                "Patch",
                stale_right,
                RapidHashSet::from_iter(["right".to_string()]),
            )
            .await
            .expect("update right");

        let final_doc = mutator
            .get_for_update("Patch", &created.doc_id)
            .await
            .expect("fetch final")
            .expect("final document");
        assert_eq!(final_doc.get("left"), Some(&NormalValue::Int(1)));
        assert_eq!(final_doc.get("right"), Some(&NormalValue::Int(1)));
    })
    .unwrap()
}

/// Two explicit transactions begin from independent copies of one document,
/// then update disjoint fields. The update implementation must validate only
/// the parts its maintenance decisions consume so both writes can commit.
#[test]
fn explicit_disjoint_field_updates_on_one_document_both_commit() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).expect("create db"));
        db.create_collection(
            test_collection()
                .with_index(IndexDescription::new("by_left").with_field("left", false)),
        )
        .await
        .expect("schema");
        let auto = AutoCommitMutator::new(Arc::clone(&db));
        let created = auto
            .create(
                "Patch",
                Document::from_json_str(r#"{"left": 0, "right": 0}"#, test_collection()).unwrap(),
            )
            .await
            .expect("create document");
        let snapshot = auto
            .get_for_update("Patch", &created.doc_id)
            .await
            .unwrap()
            .unwrap();

        let first = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        let second = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        let mut left = snapshot.clone();
        left.set("left", NormalValue::Int(1)).unwrap();
        first
            .update("Patch", left, RapidHashSet::from_iter(["left".to_string()]))
            .await
            .expect("write left");
        let mut right = snapshot;
        right.set("right", NormalValue::Int(1)).unwrap();
        second
            .update(
                "Patch",
                right,
                RapidHashSet::from_iter(["right".to_string()]),
            )
            .await
            .expect("write right");

        first
            .take_txn()
            .await
            .unwrap()
            .commit()
            .await
            .expect("first commit");
        second
            .take_txn()
            .await
            .unwrap()
            .commit()
            .await
            .expect("second commit");

        let result = auto
            .get_for_update("Patch", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.get("left"), Some(&NormalValue::Int(1)));
        assert_eq!(result.get("right"), Some(&NormalValue::Int(1)));
    })
    .unwrap()
}

#[test]
fn stale_update_uses_fresh_unmodified_composite_index_component() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).expect("create db"));
        db.create_collection(
            test_collection().with_index(
                IndexDescription::new("by_left_right")
                    .with_field("left", false)
                    .with_field("right", false),
            ),
        )
        .await
        .expect("schema");
        let mutator = AutoCommitMutator::new(Arc::clone(&db));
        let created = mutator
            .create(
                "Patch",
                Document::from_json_str(r#"{"left": 0, "right": 0}"#, test_collection()).unwrap(),
            )
            .await
            .expect("create document");
        let stale = mutator
            .get_for_update("Patch", &created.doc_id)
            .await
            .unwrap()
            .unwrap();

        let mut right_update = stale.clone();
        right_update.set("right", NormalValue::Int(1)).unwrap();
        mutator
            .update(
                "Patch",
                right_update,
                RapidHashSet::from_iter(["right".to_string()]),
            )
            .await
            .unwrap();

        let mut left_update = stale;
        left_update.set("left", NormalValue::Int(2)).unwrap();
        let explicit = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        explicit
            .update(
                "Patch",
                left_update,
                RapidHashSet::from_iter(["left".to_string()]),
            )
            .await
            .unwrap();
        explicit.take_txn().await.unwrap().commit().await.unwrap();

        let txn = db.new_txn(true).await.unwrap();
        let indexed_ids = DbDocFetcher::new(txn)
            .get_by_index_scan(
                "Patch",
                &IndexScanParams {
                    index_name: "by_left_right".to_string(),
                    scan_type: IndexScanType::ExactMatch {
                        values: vec![NormalValue::Int(2), NormalValue::Int(1)],
                    },
                    limit: None,
                    offset: 0,
                    value_filter: None,
                    cursor_seek: None,
                },
            )
            .await
            .expect("exact composite index lookup")
            .into_doc_ids();
        assert_eq!(indexed_ids, vec![created.doc_id.to_string()]);
    })
    .unwrap()
}

#[test]
fn explicit_same_indexed_field_updates_conflict_on_the_old_index_entry() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).expect("create db"));
        db.create_collection(
            test_collection()
                .with_index(IndexDescription::new("by_left").with_field("left", false)),
        )
        .await
        .expect("schema");
        let auto = AutoCommitMutator::new(Arc::clone(&db));
        let created = auto
            .create(
                "Patch",
                Document::from_json_str(r#"{"left": 0, "right": 0}"#, test_collection()).unwrap(),
            )
            .await
            .expect("create document");
        let snapshot = auto
            .get_for_update("Patch", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        let first = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        let second = DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        let mut first_doc = snapshot.clone();
        first_doc.set("left", NormalValue::Int(1)).unwrap();
        first
            .update(
                "Patch",
                first_doc,
                RapidHashSet::from_iter(["left".to_string()]),
            )
            .await
            .unwrap();
        let mut second_doc = snapshot;
        second_doc.set("left", NormalValue::Int(2)).unwrap();
        second
            .update(
                "Patch",
                second_doc,
                RapidHashSet::from_iter(["left".to_string()]),
            )
            .await
            .unwrap();
        first.take_txn().await.unwrap().commit().await.unwrap();
        assert!(second.take_txn().await.unwrap().commit().await.is_err());
    })
    .unwrap()
}

/// Query mutations carry the snapshot against which their filter was
/// validated. If that snapshot changes while the update waits for the
/// per-document guard, the mutation must conflict instead of applying a
/// predicate that is no longer known to hold.
#[test]
fn stale_conditional_update_returns_conflict() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).expect("create db"));
        db.create_collection(test_collection())
            .await
            .expect("schema");
        let mutator = AutoCommitMutator::new(Arc::clone(&db));

        let initial = Document::from_json_str(r#"{"left": 0, "right": 0}"#, test_collection())
            .expect("initial document");
        let created = mutator.create("Patch", initial).await.expect("create");

        let mut first = mutator
            .get_for_update("Patch", &created.doc_id)
            .await
            .expect("fetch first copy")
            .expect("first copy");
        let expected_second = first.clone();
        let mut second = first.clone();

        first.set("left", NormalValue::Int(1)).unwrap();
        mutator
            .update(
                "Patch",
                first,
                RapidHashSet::from_iter(["left".to_string()]),
            )
            .await
            .expect("update left");

        second.set("right", NormalValue::Int(1)).unwrap();
        let error = mutator
            .update_if_unchanged(
                "Patch",
                expected_second,
                second,
                RapidHashSet::from_iter(["right".to_string()]),
            )
            .await
            .expect_err("stale conditional update must conflict");
        assert!(matches!(
            error,
            query::error::QueryError::TransactionConflict(ref message)
                if message == "transaction conflict. Please retry"
        ));

        let final_doc = mutator
            .get_for_update("Patch", &created.doc_id)
            .await
            .expect("fetch final")
            .expect("final document");
        assert_eq!(final_doc.get("left"), Some(&NormalValue::Int(1)));
        assert_eq!(final_doc.get("right"), Some(&NormalValue::Int(0)));
    })
    .unwrap()
}

/// A delete, local or merged from a peer, can commit after the query plan
/// materialized the document but before the update reloads it under the
/// per-document guard. That is the same stale snapshot as a concurrent
/// field change, so it must conflict and let the retry observe the deletion.
#[test]
fn conditional_update_of_concurrently_deleted_document_returns_conflict() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).expect("create db"));
        db.create_collection(test_collection())
            .await
            .expect("schema");
        let mutator = AutoCommitMutator::new(Arc::clone(&db));

        let initial = Document::from_json_str(r#"{"left": 0, "right": 0}"#, test_collection())
            .expect("initial document");
        let created = mutator.create("Patch", initial).await.expect("create");

        let expected = mutator
            .get_for_update("Patch", &created.doc_id)
            .await
            .expect("fetch snapshot")
            .expect("snapshot");
        let mut patched = expected.clone();
        patched.set("right", NormalValue::Int(1)).unwrap();

        mutator
            .delete("Patch", &created.doc_id)
            .await
            .expect("concurrent delete");

        let error = mutator
            .update_if_unchanged(
                "Patch",
                expected,
                patched,
                RapidHashSet::from_iter(["right".to_string()]),
            )
            .await
            .expect_err("update of a document deleted after its snapshot must conflict");
        assert!(
            matches!(
                error,
                query::error::QueryError::TransactionConflict(ref message)
                    if message == "transaction conflict. Please retry"
            ),
            "unexpected error: {error:?}"
        );
    })
    .unwrap()
}

/// Inject an external committed write immediately before the next mutation
/// transaction opens. This bypasses the application document queue, as a
/// storage-level writer can, and pins the old read-before-begin race.
struct ChangeBeforeBegin {
    inner: RegolithStore,
    pending: AtomOption<(Vec<u8>, Vec<u8>)>,
}

impl storage::corekv::private::Sealed for ChangeBeforeBegin {}

#[async_trait::async_trait]
impl storage::corekv::Store for ChangeBeforeBegin {
    fn crdts(&self) -> &storage::Crdts {
        self.inner.crdts()
    }

    async fn allocate(&self, key: &[u8], count: u64) -> storage::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> storage::Result<Box<dyn storage::Txn>> {
        let pending = if readonly {
            None
        } else {
            self.pending.take().map(|pending| (*pending).clone())
        };
        if let Some((key, value)) = pending {
            let mut change = self.inner.new_txn(false).await?;
            change.set(&key, &value).await?;
            change.commit().await?;
        }
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: storage::corekv::RetryOptions,
        attempt: storage::corekv::RetryAttempt,
    ) -> std::result::Result<storage::corekv::RetryAttemptValue, storage::corekv::RetryAttemptError>
    {
        let pending = self.pending.take().map(|pending| (*pending).clone());
        if let Some((key, value)) = pending {
            let mut change = self.inner.new_txn(false).await?;
            change.set(&key, &value).await?;
            change.commit().await?;
        }
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> storage::Result<()> {
        self.inner.close().await
    }
}

#[test]
fn update_reads_fresh_values_in_the_transaction_that_writes_them() {
    workers::block_on(async {
        let store = Arc::new(ChangeBeforeBegin {
            inner: RegolithStore::in_memory().unwrap(),
            pending: AtomOption::none(),
        });
        let db = Arc::new(DB::open_from_arc(store.clone()).await.unwrap());
        db.create_collection(test_collection()).await.unwrap();
        let mutator = AutoCommitMutator::new(db.clone());
        let created = mutator
            .create(
                "Patch",
                Document::from_json_str(r#"{"left":0,"right":0}"#, test_collection()).unwrap(),
            )
            .await
            .unwrap();
        let collection = db.get_collection("Patch").unwrap().unwrap();
        let txn = db.new_txn(true).await.unwrap();
        let short_id = collection
            .resolve_doc_short_id(&txn.systemstore().unwrap(), &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        txn.discard().unwrap();
        let mut key = vec![b'd'];
        key.extend(collection.doc_key(short_id));
        let mut external = created.document.clone();
        external.set("left", 7_i64).unwrap();
        store.pending.store_some((key, external.to_cbor().unwrap()));
        let mut patch = created.document;
        patch.set("right", 9_i64).unwrap();
        mutator
            .update("Patch", patch, ["right".to_string()].into_iter().collect())
            .await
            .unwrap();
        let result = mutator
            .get_for_update("Patch", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.get("left"), Some(&NormalValue::Int(7)));
        assert_eq!(result.get("right"), Some(&NormalValue::Int(9)));
        assert!(store.pending.is_none());
    })
    .unwrap()
}
