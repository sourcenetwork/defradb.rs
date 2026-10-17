//! A counter increment is a blind operand on the counter's part of the
//! document value, its only stored copy. Increments of one counter, and an
//! increment racing an update of another field, never conflict.

use db::{AutoCommitMutator, LensedAutoCommitFetcher, DB};
use document::{DocID, Document, NormalValue};
use query::mutator::DocMutator;
use query::runner::DocFetcher;
use rapidhash::RapidHashSet;
use schema::{CType, CollectionVersion, FieldDescription, FieldKind};
use std::sync::Arc;
use storage::corekv::{IterOptions, Store};
use storage::RegolithStore;

const WRITERS: i64 = 64;
const LABELS: usize = 4;

/// The collection, its `count` a counter of `kind`.
fn tally(kind: CType) -> CollectionVersion {
    let mut fields = vec![
        FieldDescription::new("1", "_docID", FieldKind::doc_id()),
        FieldDescription::new("2", "count", FieldKind::int()).with_crdt_type(kind),
    ];
    for label in 0..LABELS {
        fields.push(FieldDescription::new(
            (3 + label).to_string(),
            format!("label{label}"),
            FieldKind::string(),
        ));
    }
    CollectionVersion::new("Tally", "tally-v1", "col-tally", fields)
}

async fn open() -> (Arc<RegolithStore>, Arc<DB<RegolithStore>>, DocID) {
    open_with(CType::PnCounter).await
}

/// One document at `count: 0`, its counter of `kind`.
async fn open_with(kind: CType) -> (Arc<RegolithStore>, Arc<DB<RegolithStore>>, DocID) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.create_collection(tally(kind)).await.unwrap();
    let doc_id = AutoCommitMutator::new(db.clone())
        .create("Tally", Document::from_json_str(r#"{"count": 0}"#).unwrap())
        .await
        .unwrap()
        .doc_id;
    (store, db, doc_id)
}

fn conflicts(store: &RegolithStore) -> u64 {
    store
        .transaction_stats_handle()
        .expect("regolith reports its transaction statistics")
        .snapshot()
        .conflicts
}

async fn increment(mutator: &AutoCommitMutator<RegolithStore>, doc_id: &DocID, by: i64) {
    let mut doc = Document::new();
    doc.set_id(doc_id.clone());
    doc.set_counter_delta("count".to_string(), NormalValue::Int(by));
    mutator
        .update("Tally", doc, RapidHashSet::from_iter(["count".to_string()]))
        .await
        .expect("an increment commits");
}

async fn read(db: &Arc<DB<RegolithStore>>) -> Document {
    let mut docs = LensedAutoCommitFetcher::new(db.clone())
        .get_all("Tally")
        .await
        .unwrap();
    assert_eq!(docs.len(), 1);
    docs.remove(0)
}

#[test]
fn sixty_four_concurrent_increments_and_other_field_updates_never_conflict() {
    workers::block_on(async {
        let (store, db, doc_id) = open().await;
        let before = conflicts(&store);
        let start = Arc::new(regolith::sync::Barrier::new(WRITERS as usize + LABELS));

        let mut tasks = Vec::new();
        for _ in 0..WRITERS {
            let (db, doc_id, start) = (db.clone(), doc_id.clone(), start.clone());
            tasks.push(
                workers::spawn(None, "test", move || async move {
                    let mutator = AutoCommitMutator::new(db);
                    start.wait().await;
                    increment(&mutator, &doc_id, 1).await;
                })
                .unwrap(),
            );
        }
        for label in 0..LABELS {
            let (db, doc_id, start) = (db.clone(), doc_id.clone(), start.clone());
            tasks.push(
                workers::spawn(None, "test", move || async move {
                    let mutator = AutoCommitMutator::new(db);
                    let field = format!("label{label}");
                    let mut doc = Document::new();
                    doc.set_id(doc_id);
                    doc.set(
                        field.clone(),
                        NormalValue::String(format!("set by {label}")),
                    );
                    start.wait().await;
                    mutator
                        .update("Tally", doc, RapidHashSet::from_iter([field]))
                        .await
                        .expect("an update of another field commits");
                })
                .unwrap(),
            );
        }
        for task in tasks {
            task.await.unwrap();
        }

        assert_eq!(
            conflicts(&store) - before,
            0,
            "no increment and no update of another field may conflict"
        );
        let doc = read(&db).await;
        assert_eq!(doc.get("count"), Some(&NormalValue::Int(WRITERS)));
        for label in 0..LABELS {
            assert_eq!(
                doc.get(&format!("label{label}")),
                Some(&NormalValue::String(format!("set by {label}")))
            );
        }
    })
    .unwrap()
}

/// The counter has one stored copy: no separate accumulator key is written
/// beside the document value.
#[test]
fn an_increment_writes_no_second_copy_of_the_counter() {
    workers::block_on(async {
        let (store, db, doc_id) = open().await;
        let mutator = AutoCommitMutator::new(db.clone());
        increment(&mutator, &doc_id, 5).await;
        increment(&mutator, &doc_id, -2).await;
        assert_eq!(read(&db).await.get("count"), Some(&NormalValue::Int(3)));

        let txn = store.new_txn(true).await.unwrap();
        let mut accumulators = txn
            .iterator(IterOptions::new().with_prefix(b"d/data/".to_vec()))
            .await
            .unwrap();
        assert!(accumulators.next().await.unwrap().is_none());
    })
    .unwrap()
}

/// An increment of a counter in a document whose stored value carries it
/// adds to that value: the stored value, not a precomputed absolute handed
/// in by the caller, is what the increment lands on.
#[test]
fn an_increment_lands_on_the_stored_value_not_the_callers_copy() {
    workers::block_on(async {
        let (_store, db, doc_id) = open().await;
        let mutator = AutoCommitMutator::new(db.clone());
        increment(&mutator, &doc_id, 7).await;
        // A stale caller copy that still says 0 and claims the result is 1.
        let mut stale = Document::new();
        stale.set_id(doc_id.clone());
        stale.set("count", NormalValue::Int(1));
        stale.set_counter_delta("count".to_string(), NormalValue::Int(1));
        let updated = mutator
            .update(
                "Tally",
                stale,
                RapidHashSet::from_iter(["count".to_string()]),
            )
            .await
            .unwrap();
        assert_eq!(updated.document.get("count"), Some(&NormalValue::Int(8)));
        assert_eq!(read(&db).await.get("count"), Some(&NormalValue::Int(8)));
    })
    .unwrap()
}

/// A caller's copy of the document never replaces the stored counter, for a
/// P-counter and a PN-counter alike, whether the copy says more or less than
/// is stored: only the increment moves it.
#[test]
fn a_callers_copy_never_replaces_the_stored_counter() {
    workers::block_on(async {
        for kind in [CType::PCounter, CType::PnCounter] {
            let (_store, db, doc_id) = open_with(kind).await;
            let mutator = AutoCommitMutator::new(db.clone());
            increment(&mutator, &doc_id, 5).await;
            for (copy, expected) in [(100, 6), (2, 7)] {
                let mut stale = Document::new();
                stale.set_id(doc_id.clone());
                stale.set("count", NormalValue::Int(copy));
                stale.set_counter_delta("count".to_string(), NormalValue::Int(1));
                mutator
                    .update(
                        "Tally",
                        stale,
                        RapidHashSet::from_iter(["count".to_string()]),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    read(&db).await.get("count"),
                    Some(&NormalValue::Int(expected)),
                    "{kind:?}, a copy that says {copy}"
                );
            }
        }
    })
    .unwrap()
}

/// Interactive transactions take the same route: two that increment one
/// counter and a third that updates another field, all open at once, all
/// commit, each increment applied once.
#[test]
fn interactive_increments_and_other_field_updates_all_commit() {
    workers::block_on(async {
        let (store, db, doc_id) = open().await;
        let before = conflicts(&store);
        let mut open_txns = Vec::new();
        for by in [2, 3] {
            let mutator = db::DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
            let mut doc = Document::new();
            doc.set_id(doc_id.clone());
            doc.set_counter_delta("count".to_string(), NormalValue::Int(by));
            mutator
                .update("Tally", doc, RapidHashSet::from_iter(["count".to_string()]))
                .await
                .unwrap();
            open_txns.push(mutator);
        }
        let labeler = db::DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
        let mut doc = Document::new();
        doc.set_id(doc_id.clone());
        doc.set("label0", NormalValue::String("interactive".into()));
        labeler
            .update(
                "Tally",
                doc,
                RapidHashSet::from_iter(["label0".to_string()]),
            )
            .await
            .unwrap();
        open_txns.push(labeler);
        for mutator in open_txns {
            mutator.take_txn().await.unwrap().commit().await.unwrap();
        }
        assert_eq!(conflicts(&store) - before, 0);
        let doc = read(&db).await;
        assert_eq!(doc.get("count"), Some(&NormalValue::Int(5)));
        assert_eq!(
            doc.get("label0"),
            Some(&NormalValue::String("interactive".into()))
        );
    })
    .unwrap()
}
