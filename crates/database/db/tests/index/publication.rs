//! Index changes reach the collection cache before they return, even while
//! a writer's commit hook holds back the ordered events of every later
//! commit (#2913, `proofs/tla/IndexCachePublication.tla`).

use std::sync::Arc;
use std::time::Duration;

use db::{AutoCommitMutator, DB};
use document::{Document, NormalValue};
use futures::future::{select, Either};
use query::DocMutator;
use regolith::sync::Event;
use schema::{IndexKind, IndexedFieldDescription, OrderedIndexDescription};
use storage::corekv::OnCommit;
use storage::RegolithStore;

use crate::common::schema::parsed;

const SDL: &str = "type U { g: Int }";

fn by_g() -> (Vec<IndexedFieldDescription>, IndexKind) {
    (
        vec![IndexedFieldDescription {
            name: "g".to_string(),
            descending: false,
        }],
        IndexKind::Ordered(OrderedIndexDescription { unique: false }),
    )
}

async fn open() -> Arc<DB<RegolithStore>> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    db.add_collection(&defra_core::RequestContext::ambient(), SDL)
        .await
        .unwrap();
    let mutator = AutoCommitMutator::new(db.clone());
    for g in 0..3 {
        let mut doc = Document::new(parsed(SDL));
        doc.set("g", NormalValue::Int(g)).unwrap();
        mutator.create("U", doc).await.unwrap();
    }
    db
}

/// A commit whose hook, as a writer's does, holds back the ordered events of
/// every later commit: it sets `entered` once running and waits for
/// `release`.
async fn holding_commit(
    db: &DB<RegolithStore>,
    entered: &Arc<Event>,
    release: &Arc<Event>,
) -> db::DbTxn<RegolithStore> {
    let txn = db.new_txn(false).await.unwrap();
    txn.systemstore()
        .unwrap()
        .set(b"holding", b"commit")
        .await
        .unwrap();
    let (entered, release) = (Arc::clone(entered), Arc::clone(release));
    txn.on_commit(OnCommit::OrderedAsync(Box::new(move || {
        Box::pin(async move {
            entered.set();
            release.wait().await;
            Ok(())
        })
    })))
    .unwrap();
    txn
}

/// Whether `change` was still running after a while: it waits for its cache
/// publication, which the held hook keeps from running.
async fn still_running<F>(change: std::pin::Pin<&mut F>) -> bool
where
    F: std::future::Future<Output = db::Result<()>>,
{
    let timer = workers::spawn(Some(crate::common::worker_io::pool()), "timer", || async {
        workers::sleep(Duration::from_millis(300)).unwrap().await;
    })
    .unwrap();
    match select(change, timer).await {
        Either::Left((done, _)) => {
            done.unwrap();
            false
        }
        Either::Right(_) => true,
    }
}

async fn assert_used(db: &DB<RegolithStore>, name: &str) {
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
        "index '{name}' is built but queries do not use it"
    );
}

#[test]
#[mantra_macros::req_test("defra.index.ddl_under_writes")]
fn an_index_created_while_a_commit_hook_holds_events_is_used() {
    workers::block_on(async {
        let db = open().await;
        let (entered, release) = (Arc::new(Event::new()), Arc::new(Event::new()));
        let holding = holding_commit(&db, &entered, &release).await;

        let create = async {
            entered.wait().await;
            let creating = async {
                let (fields, kind) = by_g();
                db.create_index("U", Some("by_g"), fields, kind)
                    .await
                    .map(|_| ())
            };
            futures::pin_mut!(creating);
            let waited = still_running(creating.as_mut()).await;
            release.set();
            if waited {
                creating.await.unwrap();
            }
            assert!(
                waited,
                "the create returned before its cache publication ran"
            );
            assert_used(&db, "by_g").await;
        };
        let (committed, ()) = futures::join!(holding.commit(), create);
        committed.unwrap();
        assert_used(&db, "by_g").await;
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.index.ddl_under_writes")]
fn an_index_dropped_while_a_commit_hook_holds_events_can_be_created_again() {
    workers::block_on(async {
        let db = open().await;
        let (fields, kind) = by_g();
        db.create_index("U", Some("by_g"), fields, kind)
            .await
            .unwrap();
        let (entered, release) = (Arc::new(Event::new()), Arc::new(Event::new()));
        let holding = holding_commit(&db, &entered, &release).await;

        let recreate = async {
            entered.wait().await;
            let dropping = db.drop_index("U", "by_g");
            futures::pin_mut!(dropping);
            let waited = still_running(dropping.as_mut()).await;
            release.set();
            if waited {
                dropping.await.unwrap();
            }
            assert!(waited, "the drop returned before its cache publication ran");
            assert!(db
                .require_collection("U")
                .unwrap()
                .get_index("by_g")
                .is_none());
            let (fields, kind) = by_g();
            db.create_index("U", Some("by_g"), fields, kind)
                .await
                .unwrap();
            assert_used(&db, "by_g").await;
        };
        let (committed, ()) = futures::join!(holding.commit(), recreate);
        committed.unwrap();
        assert_used(&db, "by_g").await;
    })
    .unwrap()
}
