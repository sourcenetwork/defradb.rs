use std::sync::Arc;

use db::chunk::split::{split, ChunkConfig, SplitterVersion};
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
pub(super) const CHUNKS: &str = "Article__body_chunk";
pub(super) const CONFIG: ChunkConfig = ChunkConfig {
    version: SplitterVersion::CURRENT,
    max_chars: 64,
    overlap: 0,
};

pub(super) fn blank(name: &str) -> Document {
    let collection = query::parse_sdl(SDL)
        .unwrap()
        .into_iter()
        .find(|c| c.name == name)
        .expect("collection");
    Document::new(collection)
}

pub(super) fn body(paragraphs: &[&str]) -> String {
    paragraphs.join("\n\n")
}

pub(super) fn book() -> Vec<String> {
    (0..12)
        .map(|i| format!("Paragraph {i} tells a small story of its own."))
        .collect()
}

pub(super) fn text_of(paras: &[String]) -> String {
    body(&paras.iter().map(String::as_str).collect::<Vec<_>>())
}

pub(super) async fn setup() -> (
    db::AutoCommitMutator<storage::RegolithStore>,
    Arc<db::DB<storage::RegolithStore>>,
    impl Sized,
) {
    let (db, bus) = make_test_db_with_bus().await;
    db.add_collection(&defra_core::RequestContext::ambient(), SDL)
        .await
        .expect("schema");
    (db::AutoCommitMutator::new(Arc::clone(&db)), db, bus)
}

pub(super) fn article(title: &str, text: &str) -> Document {
    let mut doc = blank("Article");
    doc.set("title", NormalValue::String(title.into())).unwrap();
    doc.set("body", NormalValue::String(text.into())).unwrap();
    doc
}

pub(super) fn modified(fields: &[&str]) -> RapidHashSet<String> {
    fields.iter().map(|f| f.to_string()).collect()
}

pub(super) async fn body_of(mutator: &impl DocMutator, doc_id: &DocID) -> Option<String> {
    let doc = mutator
        .get_for_update("Article", doc_id)
        .await
        .unwrap()
        .expect("parent");
    doc.get("body")
        .and_then(NormalValue::as_str)
        .map(str::to_owned)
}

pub(super) async fn update_body(
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
fn create_writes_the_text_and_every_chunk() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let text = text_of(&book());
        let created = mutator
            .create("Article", article("t", &text))
            .await
            .unwrap();

        let chunks = split(&text, &CONFIG);
        assert_eq!(
            body_of(&mutator, &created.doc_id).await.unwrap(),
            text.as_str()
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

        // One new chunk, and the delete of the one it replaces.
        let (made, gone): (Vec<_>, Vec<_>) = updated
            .derived
            .iter()
            .partition(|write| write.document.is_some());
        assert_eq!((made.len(), gone.len()), (1, 1));
        assert_eq!(
            body_of(&mutator, &created.doc_id).await.unwrap(),
            edited.as_str()
        );
        for old in &created.derived {
            let kept = old.doc_id != gone[0].doc_id;
            assert_eq!(mutator.exists(CHUNKS, &old.doc_id).await.unwrap(), kept);
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
        let before = body_of(&mutator, &created.doc_id).await;

        let mut doc = blank("Article");
        doc.set_id(created.doc_id.clone());
        doc.set("title", NormalValue::String("new title".into()))
            .unwrap();
        let updated = mutator
            .update("Article", doc, modified(&["title"]))
            .await
            .unwrap();

        assert!(updated.derived.is_empty());
        assert_eq!(body_of(&mutator, &created.doc_id).await, before);
    })
    .unwrap()
}

#[test]
fn reverting_the_text_deletes_the_added_chunk_and_keeps_the_rest() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let a = text_of(&book());
        let b = format!("{a}\n\nA closing paragraph added later.");
        let created = mutator.create("Article", article("t", &a)).await.unwrap();
        let source_of = |doc: &Document| doc.get("sourceCID").cloned();
        let first = source_of(created.derived[0].document.as_ref().unwrap());

        let edit = update_body(&mutator, &created.doc_id, &b).await;
        let back = update_body(&mutator, &created.doc_id, &a).await;

        // Only the chunks the edit deleted come back, under the revert's
        // commit; the untouched ones were never deleted.
        let (made, gone): (Vec<_>, Vec<_>) = back
            .derived
            .iter()
            .partition(|write| write.document.is_some());
        let edit_gone = edit
            .derived
            .iter()
            .filter(|write| write.document.is_none())
            .count();
        assert_eq!((made.len(), gone.len()), (edit_gone, 2));
        let mut added: Vec<String> = edit
            .derived
            .iter()
            .filter(|write| write.document.is_some())
            .map(|write| write.doc_id.to_string())
            .collect();
        let mut gone_ids: Vec<String> = gone.iter().map(|write| write.doc_id.to_string()).collect();
        gone_ids.sort();
        added.sort();
        assert_eq!(gone_ids, added);
        let again = source_of(made[0].document.as_ref().unwrap());
        assert!(again.is_some() && again != first, "{again:?}");
        for old in &created.derived {
            assert_ne!(made[0].doc_id, old.doc_id);
        }
        assert_eq!(
            body_of(&mutator, &created.doc_id).await.unwrap(),
            a.as_str()
        );
    })
    .unwrap()
}

#[test]
fn a_null_text_has_no_chunks() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let mut doc = blank("Article");
        doc.set("title", NormalValue::String("t".into())).unwrap();
        doc.set("body", NormalValue::Null).unwrap();
        let created = mutator.create("Article", doc).await.unwrap();

        assert!(created.derived.is_empty());
        assert_eq!(body_of(&mutator, &created.doc_id).await, None);
    })
    .unwrap()
}

#[test]
fn deleting_the_parent_deletes_every_chunk_of_its_text() {
    workers::block_on(async {
        let (mutator, _db, _keep) = setup().await;
        let mut paras = book();
        let created = mutator
            .create("Article", article("t", &text_of(&paras)))
            .await
            .unwrap();
        paras[3] = "Paragraph 3 was rewritten before the delete.".into();
        let updated = update_body(&mutator, &created.doc_id, &text_of(&paras)).await;
        let mut live: Vec<String> = created
            .derived
            .iter()
            .chain(&updated.derived)
            .filter(|write| write.document.is_some())
            .filter(|write| {
                !updated
                    .derived
                    .iter()
                    .any(|gone| gone.document.is_none() && gone.doc_id == write.doc_id)
            })
            .map(|write| write.doc_id.to_string())
            .collect();
        live.sort();

        let deleted = mutator.delete("Article", &created.doc_id).await.unwrap();

        let mut gone: Vec<String> = deleted
            .derived
            .iter()
            .map(|w| w.doc_id.to_string())
            .collect();
        gone.sort();
        assert_eq!(gone, live);
        for write in &deleted.derived {
            assert!(!mutator.exists(CHUNKS, &write.doc_id).await.unwrap());
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
fn users_cannot_write_chunks() {
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
        assert_eq!(body_of(&txn, &created.doc_id).await.unwrap(), text.as_str());
        txn.take_txn().await.unwrap().commit().await.unwrap();
        assert_eq!(
            body_of(&autocommit, &created.doc_id).await.unwrap(),
            text.as_str()
        );

        assert!(!created.derived.is_empty());
        for write in &created.derived {
            assert!(autocommit.exists(CHUNKS, &write.doc_id).await.unwrap());
        }

        let txn = db::DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        txn.delete("Article", &created.doc_id).await.unwrap();
        // Inside the transaction the chunks are gone; outside they stay until it commits.
        for write in &created.derived {
            assert!(!txn.exists(CHUNKS, &write.doc_id).await.unwrap());
            assert!(autocommit.exists(CHUNKS, &write.doc_id).await.unwrap());
        }
        txn.take_txn().await.unwrap().commit().await.unwrap();
        let doc = autocommit
            .get_for_update("Article", &created.doc_id)
            .await
            .unwrap();
        assert!(doc.is_none() || doc.unwrap().is_deleted());
        for write in &created.derived {
            assert!(!autocommit.exists(CHUNKS, &write.doc_id).await.unwrap());
        }
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
        let made = updated
            .derived
            .iter()
            .filter(|write| write.document.is_some())
            .count();
        assert_eq!(made, 1);
        assert_eq!(
            body_of(&mutator, &created.doc_id).await.unwrap(),
            "Rewritten."
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
        db.add_collection(&defra_core::RequestContext::ambient(), sdl)
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
