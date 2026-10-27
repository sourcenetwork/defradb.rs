use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use db::database::{DbOptions, DB};
use db::AutoCommitMutator;
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use regolith::sync::OnceCell;

use crate::common::failing_write_store::FailingWriteStore;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;

fn fields(names: &[&str]) -> RapidHashSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn a_checked_update_of_the_text_ignores_a_concurrent_title_write() {
    workers::block_on(async {
        let node: Arc<OnceCell<Arc<DB<FailingWriteStore>>>> = Arc::new(OnceCell::new());
        let raced = Arc::new(AtomicBool::new(false));
        // The first write this update stages commits a title change first, so
        // the update's earlier read is now stale for the title only.
        let store = FailingWriteStore::in_memory(Arc::new({
            let (node, raced) = (Arc::clone(&node), Arc::clone(&raced));
            move |_key, _value| {
                let Some(db) = node.get() else { return Ok(()) };
                if raced.swap(true, Ordering::SeqCst) {
                    return Ok(());
                }
                let db = Arc::clone(db);
                std::thread::spawn(move || {
                    workers::block_on(async move {
                        let mutator = AutoCommitMutator::new(Arc::clone(&db));
                        let (_, doc_id) = article(&db).await;
                        let mut doc = blank(&db);
                        doc.set_id(doc_id);
                        doc.set("title", NormalValue::String("renamed".into()))
                            .unwrap();
                        mutator.update("Article", doc, fields(&["title"])).await
                    })
                    .unwrap()
                })
                .join()
                .unwrap()
                .unwrap();
                Ok(())
            }
        }));
        let options = DbOptions::default().with_max_txn_retries(0);
        let db = Arc::new(DB::with_options(store, options).unwrap());
        db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
            .await
            .unwrap();
        let mutator = AutoCommitMutator::new(Arc::clone(&db));
        let mut first = blank(&db);
        first
            .set("title", NormalValue::String("draft".into()))
            .unwrap();
        first
            .set("body", NormalValue::String("The first text.".into()))
            .unwrap();
        mutator.create("Article", first).await.unwrap();
        let (selected, doc_id) = article(&db).await;

        let _ = node.set(Arc::clone(&db));
        let mut doc = blank(&db);
        doc.set_id(doc_id);
        doc.set("body", NormalValue::String("A new text.".into()))
            .unwrap();
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

        assert!(raced.load(Ordering::SeqCst));
    })
    .unwrap()
}

fn blank(db: &DB<FailingWriteStore>) -> Document {
    Document::new(
        db.get_collection("Article")
            .unwrap()
            .unwrap()
            .schema()
            .clone(),
    )
}

async fn article(db: &Arc<DB<FailingWriteStore>>) -> (Document, document::DocID) {
    let mutator = AutoCommitMutator::new(Arc::clone(db));
    let docs = db::LensedAutoCommitFetcher::new(Arc::clone(db));
    let all = query::DocFetcher::get_all(&docs, "Article").await.unwrap();
    let id = all[0].id().unwrap().clone();
    let doc = mutator
        .get_for_update("Article", &id)
        .await
        .unwrap()
        .unwrap();
    (doc, id)
}
