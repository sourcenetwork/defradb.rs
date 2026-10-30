use std::sync::Arc;

use db::chunk::list::chunk_list;
use db::chunk::split::{split, ChunkConfig};
use db::write::autocommit::batch::BatchMutator;
use document::{DocID, Document, NormalValue};
use query::mutator::{DocMutator, MutationBatchController};
use rapidhash::RapidHashSet;

use crate::common::fixture::make_test_db_with_bus;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;
const CHUNKS: &str = "Article__body_chunk";
const CONFIG: ChunkConfig = ChunkConfig {
    max_chars: 64,
    overlap: 0,
};

fn blank(name: &str) -> Document {
    let collection = query::parse_sdl(SDL)
        .unwrap()
        .into_iter()
        .find(|c| c.name == name)
        .expect("collection");
    Document::new(collection)
}

fn body(paragraphs: &[&str]) -> String {
    paragraphs.join("\n\n")
}

fn book() -> Vec<String> {
    (0..12)
        .map(|i| format!("Paragraph {i} tells a small story of its own."))
        .collect()
}

fn text_of(paras: &[String]) -> String {
    body(&paras.iter().map(String::as_str).collect::<Vec<_>>())
}

async fn setup() -> (
    db::AutoCommitMutator<storage::RegolithStore>,
    Arc<db::DB<storage::RegolithStore>>,
    impl Sized,
) {
    let (db, bus) = make_test_db_with_bus().await;
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .expect("schema");
    (db::AutoCommitMutator::new(Arc::clone(&db)), db, bus)
}

fn article(title: &str, text: &str) -> Document {
    let mut doc = blank("Article");
    doc.set("title", NormalValue::String(title.into())).unwrap();
    doc.set("body", NormalValue::String(text.into())).unwrap();
    doc
}

fn modified(fields: &[&str]) -> RapidHashSet<String> {
    fields.iter().map(|f| f.to_string()).collect()
}

async fn list_of(mutator: &impl DocMutator, doc_id: &DocID) -> Option<Vec<String>> {
    let doc = mutator
        .get_for_update("Article", doc_id)
        .await
        .unwrap()
        .expect("parent");
    match doc.get("_body_chunks") {
        Some(NormalValue::StringArray(list)) => Some(list.clone()),
        Some(NormalValue::NillableStringArray(Some(list))) => Some(list.clone()),
        _ => None,
    }
}

async fn update_body(
    mutator: &impl DocMutator,
    doc_id: &DocID,
    text: &str,
) -> query::mutator::UpdateResult {
    let mut doc = blank("Article");
    doc.set_id(doc_id.clone());
    doc.set("body", NormalValue::String(text.into())).unwrap();
    mutator
        .update("Article", doc, modified(&["body"]))
        .await
        .expect("update")
}

#[test]
fn create_writes_the_list_and_every_chunk() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let text = text_of(&book());
        let created = mutator
            .create("Article", article("t", &text))
            .await
            .unwrap();

        let chunks = split(&text, &CONFIG);
        assert_eq!(
            list_of(&mutator, &created.doc_id).await.unwrap(),
            chunk_list(&text, &chunks)
        );
        assert_eq!(created.derived.len(), chunks.len());
        for (write, chunk) in created.derived.iter().zip(&chunks) {
            assert_eq!(write.collection_name, CHUNKS);
            let doc = write.document.as_ref().unwrap();
            assert_eq!(
                doc.get("text").and_then(NormalValue::as_str),
                Some(chunk.text.as_str())
            );
            assert_eq!(
                doc.get("hash").and_then(NormalValue::as_str),
                Some(chunk.hash.as_str())
            );
            assert_eq!(
                doc.get("_sourceID").and_then(NormalValue::as_str),
                Some(created.doc_id.to_string().as_str())
            );
            assert!(mutator.exists(CHUNKS, &write.doc_id).await.unwrap());
        }
    })
    .unwrap()
}

#[test]
fn editing_one_paragraph_writes_only_its_chunk() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let mut paras = book();
        let created = mutator
            .create("Article", article("t", &text_of(&paras)))
            .await
            .unwrap();
        paras[6] = "Paragraph 6 tells a small story of its owm.".into();
        let edited = text_of(&paras);

        let updated = update_body(&mutator, &created.doc_id, &edited).await;

        assert_eq!(updated.derived.len(), 1);
        assert_eq!(
            list_of(&mutator, &created.doc_id).await.unwrap(),
            chunk_list(&edited, &split(&edited, &CONFIG))
        );
        for old in &created.derived {
            assert!(mutator.exists(CHUNKS, &old.doc_id).await.unwrap());
        }
    })
    .unwrap()
}

#[test]
fn an_update_that_leaves_the_text_alone_writes_no_chunk() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let text = text_of(&book());
        let created = mutator
            .create("Article", article("t", &text))
            .await
            .unwrap();
        let before = list_of(&mutator, &created.doc_id).await;

        let mut doc = blank("Article");
        doc.set_id(created.doc_id.clone());
        doc.set("title", NormalValue::String("new title".into()))
            .unwrap();
        let updated = mutator
            .update("Article", doc, modified(&["title"]))
            .await
            .unwrap();

        assert!(updated.derived.is_empty());
        assert_eq!(list_of(&mutator, &created.doc_id).await, before);
    })
    .unwrap()
}

#[test]
fn reverting_the_text_reuses_the_old_chunks() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let a = text_of(&book());
        let b = format!("{a}\n\nA closing paragraph added later.");
        let created = mutator.create("Article", article("t", &a)).await.unwrap();

        assert!(!update_body(&mutator, &created.doc_id, &b)
            .await
            .derived
            .is_empty());
        let back = update_body(&mutator, &created.doc_id, &a).await;

        assert!(back.derived.is_empty());
        assert_eq!(
            list_of(&mutator, &created.doc_id).await.unwrap(),
            chunk_list(&a, &split(&a, &CONFIG))
        );
    })
    .unwrap()
}

#[test]
fn a_null_text_has_a_null_list_and_no_chunks() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let mut doc = blank("Article");
        doc.set("title", NormalValue::String("t".into())).unwrap();
        doc.set("body", NormalValue::Null).unwrap();
        let created = mutator.create("Article", doc).await.unwrap();

        assert!(created.derived.is_empty());
        assert_eq!(list_of(&mutator, &created.doc_id).await, None);
    })
    .unwrap()
}

#[test]
fn deleting_the_parent_deletes_every_chunk() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let a = text_of(&book());
        let created = mutator.create("Article", article("t", &a)).await.unwrap();
        let b = format!("Something new at the start.\n\n{a}");
        let updated = update_body(&mutator, &created.doc_id, &b).await;
        let all: Vec<DocID> = created
            .derived
            .iter()
            .chain(&updated.derived)
            .map(|w| w.doc_id.clone())
            .collect();

        let deleted = mutator.delete("Article", &created.doc_id).await.unwrap();

        assert_eq!(deleted.derived.len(), all.len());
        for doc_id in &all {
            assert!(!mutator.exists(CHUNKS, doc_id).await.unwrap());
        }
    })
    .unwrap()
}

#[test]
fn purging_parents_deletes_their_chunks() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let first = mutator
            .create("Article", article("a", &text_of(&book())))
            .await
            .unwrap();
        let second = mutator
            .create("Article", article("b", "A short text of its own."))
            .await
            .unwrap();

        let deleted = mutator
            .delete_many_impl("Article", &[first.doc_id.clone(), second.doc_id.clone()])
            .await
            .unwrap();

        assert!(deleted.iter().all(|result| result.existed));
        for write in first.derived.iter().chain(&second.derived) {
            assert!(!mutator.exists(CHUNKS, &write.doc_id).await.unwrap());
        }
    })
    .unwrap()
}

#[test]
fn users_cannot_write_chunks_or_the_list() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let created = mutator
            .create("Article", article("t", &text_of(&book())))
            .await
            .unwrap();
        let chunk = created.derived[0].doc_id.clone();

        let mut doc = blank(CHUNKS);
        doc.set("text", NormalValue::String("forged".into()))
            .unwrap();
        let err = mutator.create(CHUNKS, doc).await.unwrap_err().to_string();
        assert!(err.contains("written only by writes to Article"), "{err}");

        let mut doc = blank(CHUNKS);
        doc.set_id(chunk.clone());
        doc.set("text", NormalValue::String("forged".into()))
            .unwrap();
        let err = mutator
            .update(CHUNKS, doc, modified(&["text"]))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("written only by writes to Article"), "{err}");

        let err = mutator
            .delete(CHUNKS, &chunk)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("written only by writes to Article"), "{err}");

        let mut doc = blank("Article");
        doc.set_id(created.doc_id.clone());
        doc.set(
            "_body_chunks",
            NormalValue::StringArray(vec!["forged".into()]),
        )
        .unwrap();
        let err = mutator
            .update("Article", doc, modified(&["_body_chunks"]))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("read-only"), "{err}");
    })
    .unwrap()
}

#[test]
fn an_explicit_batch_writes_chunks_in_its_transaction() {
    workers::block_on(async {
        let (mutator, db, _keep) = setup().await;
        let text = text_of(&book());
        let txn = db.new_txn(false).await.expect("txn");
        let batch = BatchMutator::new(Arc::clone(&db), Arc::new(txn)).await;
        let created = batch.create("Article", article("t", &text)).await.unwrap();
        assert!(!created.derived.is_empty());
        assert!(!mutator
            .exists(CHUNKS, &created.derived[0].doc_id)
            .await
            .unwrap());

        batch.commit().await.expect("commit");

        for write in &created.derived {
            assert!(mutator.exists(CHUNKS, &write.doc_id).await.unwrap());
        }
    })
    .unwrap()
}

#[test]
fn an_explicit_transaction_writes_and_deletes_chunks() {
    workers::block_on(async {
        let (autocommit, db, _keep) = setup().await;
        let text = text_of(&book());
        let txn = db::DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        let created = txn.create("Article", article("t", &text)).await.unwrap();
        let chunks = split(&text, &CONFIG);
        assert_eq!(
            list_of(&txn, &created.doc_id).await.unwrap(),
            chunk_list(&text, &chunks)
        );
        txn.take_txn().await.unwrap().commit().await.unwrap();
        assert_eq!(
            list_of(&autocommit, &created.doc_id).await.unwrap(),
            chunk_list(&text, &chunks)
        );

        let txn = db::DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        txn.delete("Article", &created.doc_id).await.unwrap();
        txn.take_txn().await.unwrap().commit().await.unwrap();
        let doc = autocommit
            .get_for_update("Article", &created.doc_id)
            .await
            .unwrap();
        assert!(doc.is_none() || doc.unwrap().is_deleted());
    })
    .unwrap()
}

#[test]
fn an_update_of_a_document_that_changed_since_it_was_selected_is_refused() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let created = mutator
            .create("Article", article("draft", &text_of(&book())))
            .await
            .unwrap();
        let selected = mutator
            .get_for_update("Article", &created.doc_id)
            .await
            .unwrap()
            .expect("parent");

        let mut doc = blank("Article");
        doc.set_id(created.doc_id.clone());
        doc.set("title", NormalValue::String("published".into()))
            .unwrap();
        mutator
            .update("Article", doc, modified(&["title"]))
            .await
            .unwrap();

        let mut doc = blank("Article");
        doc.set_id(created.doc_id.clone());
        doc.set("body", NormalValue::String("Rewritten.".into()))
            .unwrap();
        let err = mutator
            .update_if_unchanged(
                "Article",
                selected.clone(),
                doc.clone(),
                modified(&["body"]),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("conflict"), "{err}");
        let err = mutator
            .update_if_fields_unchanged(
                "Article",
                selected,
                modified(&["title"]),
                doc,
                modified(&["body"]),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("conflict"), "{err}");

        let current = mutator
            .get_for_update("Article", &created.doc_id)
            .await
            .unwrap()
            .expect("parent");
        assert_eq!(
            current.get("body").and_then(NormalValue::as_str),
            Some(text_of(&book()).as_str())
        );
    })
    .unwrap()
}

#[test]
fn an_update_of_an_unchanged_document_goes_through() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let created = mutator
            .create("Article", article("draft", &text_of(&book())))
            .await
            .unwrap();
        let selected = mutator
            .get_for_update("Article", &created.doc_id)
            .await
            .unwrap()
            .expect("parent");

        let mut doc = blank("Article");
        doc.set_id(created.doc_id.clone());
        doc.set("body", NormalValue::String("Rewritten.".into()))
            .unwrap();
        let updated = mutator
            .update_if_unchanged("Article", selected, doc, modified(&["body"]))
            .await
            .unwrap();
        assert_eq!(updated.derived.len(), 1);
        assert_eq!(
            list_of(&mutator, &created.doc_id).await.unwrap(),
            chunk_list("Rewritten.", &split("Rewritten.", &CONFIG))
        );
    })
    .unwrap()
}

#[test]
fn two_chunked_fields_with_the_same_text_both_get_their_chunks() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        let sdl = r#"type Post {
            title: String @chunk(maxChars: 64)
            body: String @chunk(maxChars: 64)
        }"#;
        db.create_collections_atomic(query::parse_sdl(sdl).unwrap())
            .await
            .unwrap();
        let schema = db.get_collection("Post").unwrap().unwrap().schema().clone();
        let doc =
            Document::from_json_str(r#"{"title": "Same text.", "body": "Same text."}"#, schema)
                .unwrap();

        let mutator = db::AutoCommitMutator::new(Arc::clone(&db));
        let created = mutator.create("Post", doc).await.unwrap();

        let mut chunks: Vec<_> = created
            .derived
            .iter()
            .map(|write| write.collection_name.as_str())
            .collect();
        chunks.sort();
        assert_eq!(chunks, ["Post__body_chunk", "Post__title_chunk"]);
        for write in &created.derived {
            assert!(mutator
                .exists(&write.collection_name, &write.doc_id)
                .await
                .unwrap());
        }
    })
    .unwrap()
}
