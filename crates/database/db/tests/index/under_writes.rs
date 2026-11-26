//! An index created, or dropped and created again, while updates run on its
//! collection is built and used (#2913).

use rapidhash::RapidHashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use db::{AutoCommitMutator, DB};
use document::{DocID, Document, NormalValue};
use query::DocMutator;
use schema::{CollectionVersion, IndexKind, IndexedFieldDescription, OrderedIndexDescription};
use storage::RegolithStore;

use crate::common::schema::parsed;

const SDL: &str = r#"type U { n: Int @crdt(type: "pcounter") g: Int }"#;
const DOCS: usize = 500;
const WRITERS: usize = 8;

fn schema() -> CollectionVersion {
    parsed(SDL)
}

fn on(field: &str) -> (Vec<IndexedFieldDescription>, IndexKind) {
    (
        vec![IndexedFieldDescription {
            name: field.to_string(),
            descending: false,
        }],
        IndexKind::Ordered(OrderedIndexDescription { unique: false }),
    )
}

async fn open() -> (Arc<DB<RegolithStore>>, Vec<DocID>) {
    let db = Arc::new(DB::from_arc(Arc::new(RegolithStore::in_memory().unwrap())).unwrap());
    db.add_collection(&defra_core::RequestContext::ambient(), SDL)
        .await
        .unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    let mut ids = Vec::with_capacity(DOCS);
    for i in 0..DOCS {
        let mut doc = Document::new(schema());
        doc.set("n", NormalValue::Int(0)).unwrap();
        doc.set("g", NormalValue::Int(i as i64)).unwrap();
        ids.push(mutator.create("U", doc).await.unwrap().doc_id);
    }
    (db, ids)
}

/// Start `WRITERS` tasks updating `n` and `g` of every document in turn
/// until `stop` is set. A write that loses a conflict is simply skipped.
fn start_writers(
    db: &Arc<DB<RegolithStore>>,
    ids: &[DocID],
    stop: &Arc<AtomicBool>,
) -> Vec<workers::Completion<()>> {
    (0..WRITERS)
        .map(|writer| {
            let mutator = AutoCommitMutator::new(db.clone());
            let ids = ids.to_vec();
            let stop = stop.clone();
            workers::spawn(
                Some(crate::common::worker_io::pool()),
                "writer",
                move || async move {
                    let mut round = writer;
                    while !stop.load(Ordering::Relaxed) {
                        let id = &ids[round % ids.len()];
                        let mut doc = Document::new(schema());
                        doc.set_id(id.clone());
                        doc.set("n", NormalValue::Int(1)).unwrap();
                        doc.set("g", NormalValue::Int((round * 7919 % 1000) as i64))
                            .unwrap();
                        let _ = mutator
                            .update(
                                "U",
                                doc,
                                RapidHashSet::from_iter(["n".to_string(), "g".to_string()]),
                            )
                            .await;
                        round += WRITERS;
                        workers::yield_now().await;
                    }
                },
            )
            .unwrap()
        })
        .collect()
}

async fn assert_built(db: &DB<RegolithStore>, name: &str) {
    let collection = db.require_collection("U").unwrap();
    let index = collection
        .get_index(name)
        .unwrap_or_else(|| panic!("index '{name}' is not defined"))
        .clone();
    let actions = db
        .list_index_actions(collection.collection_id())
        .await
        .unwrap();
    assert!(
        !actions.contains_key(&index.id),
        "the backfill of '{name}' did not complete: {:?}",
        actions.get(&index.id)
    );
    assert!(
        db.query_schema("U")
            .unwrap()
            .indexes
            .iter()
            .any(|queryable| queryable.id == index.id),
        "index '{name}' is not used by queries"
    );
}

fn run<F, Fut>(steps: F)
where
    F: FnOnce(Arc<DB<RegolithStore>>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    workers::block_on(async {
        let (db, ids) = open().await;
        let (fields, kind) = on("n");
        db.create_index("U", Some("by_n"), fields, kind)
            .await
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let writers = start_writers(&db, &ids, &stop);
        let stepper = db.clone();
        workers::spawn(Some(crate::common::worker_io::pool()), "steps", move || {
            steps(stepper)
        })
        .unwrap()
        .await
        .unwrap();
        stop.store(true, Ordering::Relaxed);
        for writer in writers {
            writer.await.unwrap();
        }
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.index.ddl_under_writes")]
fn an_index_recreated_while_updates_run_is_built() {
    run(|db| async move {
        for field in ["n", "g", "n"] {
            db.drop_index("U", "by_n").await.unwrap();
            let (fields, kind) = on(field);
            db.create_index("U", Some("by_n"), fields, kind)
                .await
                .unwrap();
            assert_built(&db, "by_n").await;
        }
    })
}

#[test]
#[mantra_macros::req_test("defra.index.ddl_under_writes")]
fn a_second_index_created_while_updates_run_is_built() {
    run(|db| async move {
        let (fields, kind) = on("g");
        db.create_index("U", Some("by_g"), fields, kind)
            .await
            .unwrap();
        assert_built(&db, "by_g").await;
        assert_built(&db, "by_n").await;
    })
}
