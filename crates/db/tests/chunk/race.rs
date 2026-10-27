use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use db::database::{DbOptions, DB};
use db::AutoCommitMutator;
use document::{DocID, Document, NormalValue};
use query::mutator::DocMutator;
use query::Filter;
use rapidhash::RapidHashSet;
use regolith::sync::OnceCell;

use crate::common::failing_write_store::FailingWriteStore;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;

type Node = Arc<DB<FailingWriteStore>>;

/// A node where, once `armed` is set to n, the n-th write a transaction
/// stages from then on first lets `race` commit a write of its own.
async fn racing_node<F>(retries: u32, race: F) -> (Node, Arc<AtomicUsize>)
where
    F: Fn(Node) + Send + Sync + 'static,
{
    let node: Arc<OnceCell<Node>> = Arc::new(OnceCell::new());
    let armed = Arc::new(AtomicUsize::new(0));
    let race = Arc::new(race);
    let store = FailingWriteStore::in_memory(Arc::new({
        let (node, armed) = (Arc::clone(&node), Arc::clone(&armed));
        move |_key, _value| {
            let Some(db) = node.get() else { return Ok(()) };
            let left = armed.try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
            if left == Ok(1) {
                let (db, race) = (Arc::clone(db), Arc::clone(&race));
                std::thread::spawn(move || race(db)).join().unwrap();
            }
            Ok(())
        }
    }));
    let options = DbOptions::default().with_max_txn_retries(retries);
    let db = Arc::new(DB::with_options(store, options).unwrap());
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .unwrap();
    let _ = node.set(Arc::clone(&db));
    (db, armed)
}

fn fields(names: &[&str]) -> RapidHashSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn blank(db: &Node) -> Document {
    Document::new(
        db.get_collection("Article")
            .unwrap()
            .unwrap()
            .schema()
            .clone(),
    )
}

async fn create(db: &Node, title: &str, body: &str) -> DocID {
    let mut doc = blank(db);
    doc.set("title", NormalValue::String(title.into())).unwrap();
    doc.set("body", NormalValue::String(body.into())).unwrap();
    AutoCommitMutator::new(Arc::clone(db))
        .create("Article", doc)
        .await
        .unwrap()
        .doc_id
}

/// Sets the title of the only article, from another thread.
fn rename(db: Node) {
    workers::block_on(async move {
        let docs = db::LensedAutoCommitFetcher::new(Arc::clone(&db));
        let all = query::DocFetcher::get_all(&docs, "Article").await.unwrap();
        let mut doc = blank(&db);
        doc.set_id(all[0].id().unwrap().clone());
        doc.set("title", NormalValue::String("renamed".into()))
            .unwrap();
        AutoCommitMutator::new(db)
            .update("Article", doc, fields(&["title"]))
            .await
    })
    .unwrap()
    .unwrap();
}

#[test]
fn a_checked_update_of_the_text_ignores_a_concurrent_title_write() {
    workers::block_on(async {
        let (db, armed) = racing_node(0, rename).await;
        let mutator = AutoCommitMutator::new(Arc::clone(&db));
        let doc_id = create(&db, "draft", "The first text.").await;
        let selected = mutator
            .get_for_update("Article", &doc_id)
            .await
            .unwrap()
            .unwrap();

        let mut doc = blank(&db);
        doc.set_id(doc_id);
        doc.set("body", NormalValue::String("A new text.".into()))
            .unwrap();
        armed.store(1, Ordering::SeqCst);
        mutator
            .update_if_fields_unchanged(
                "Article",
                selected,
                fields(&["body"]),
                doc,
                fields(&["body"]),
            )
            .await
            .expect("a title write doesn't touch what this update read");
        assert_eq!(armed.load(Ordering::SeqCst), 0, "the title write ran");
    })
    .unwrap()
}

#[test]
fn a_filtered_truncate_runs_a_page_again_after_a_concurrent_write() {
    workers::block_on(async {
        let (db, armed) = racing_node(3, rename).await;
        create(&db, "draft", "The first text.\n\nThe second text.").await;

        let mut conditions = serde_json::Map::new();
        conditions.insert("body".into(), serde_json::json!({"_like": "%first%"}));
        // The page's last staged write, after the truncate's bookkeeping and
        // every read the page decides on.
        armed.store(44, Ordering::SeqCst);
        db.truncate_collection_with_filter("Article", Filter::from_conditions(conditions), None)
            .await
            .expect("the page runs again instead of failing the truncate");
        assert_eq!(armed.load(Ordering::SeqCst), 0, "the title write ran");

        let docs = db::LensedAutoCommitFetcher::new(Arc::clone(&db));
        for collection in ["Article", "Article__body_chunk"] {
            let left = query::DocFetcher::get_all(&docs, collection).await.unwrap();
            assert!(left.is_empty(), "{collection}: {left:?}");
        }
    })
    .unwrap()
}
