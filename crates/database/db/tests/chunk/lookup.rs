//! A write reuses only its own parent's chunks.

use std::sync::Arc;

use document::NormalValue;
use query::mutator::DocMutator;

use super::sync::{article, body_of, book, setup, update_body, CHUNKS};

#[test]
fn an_update_creates_a_chunk_another_parent_already_has() {
    workers::block_on(async {
        let (mutator, _db, _bus) = setup().await;
        let paragraphs = book();
        let shared = paragraphs[0].clone();
        let other = mutator
            .create("Article", article("other", &shared))
            .await
            .unwrap();
        let mine = mutator
            .create("Article", article("mine", &paragraphs[1]))
            .await
            .unwrap();

        let updated = update_body(
            &mutator,
            &mine.doc_id,
            &format!("{shared}\n\n{}", paragraphs[1]),
        )
        .await;

        assert_eq!(updated.derived.len(), 1);
        let chunk = updated.derived[0].document.as_ref().unwrap();
        assert_eq!(updated.derived[0].collection_name, CHUNKS);
        assert_eq!(
            chunk.get("_sourceID").and_then(NormalValue::as_str),
            Some(mine.doc_id.to_string().as_str())
        );
        assert_eq!(
            chunk.get("text").and_then(NormalValue::as_str),
            Some(format!("{shared}\n\n").as_str())
        );
        assert_ne!(other.doc_id, mine.doc_id);
    })
    .unwrap()
}

#[test]
fn a_revert_after_many_edits_reuses_the_old_chunk() {
    workers::block_on(async {
        let (mutator, _db, _bus) = setup().await;
        let first = "The first text of the article.";
        let created = mutator
            .create("Article", article("t", first))
            .await
            .unwrap();
        // More old chunks than the scan reads before it looks hashes up one by one.
        for edit in 0..20 {
            let text = format!("Edit {edit} of the article.");
            assert_eq!(
                update_body(&mutator, &created.doc_id, &text)
                    .await
                    .derived
                    .len(),
                1
            );
        }

        let back = update_body(&mutator, &created.doc_id, first).await;
        let again = update_body(&mutator, &created.doc_id, "Edit 3 of the article.").await;
        let fresh = update_body(&mutator, &created.doc_id, "A text never written before.").await;

        assert!(back.derived.is_empty());
        assert!(again.derived.is_empty());
        assert_eq!(fresh.derived.len(), 1);
    })
    .unwrap()
}

#[test]
fn an_explicit_transaction_reuses_an_old_chunk_after_many_edits() {
    workers::block_on(async {
        let (mutator, db, _bus) = setup().await;
        let first = "The first text of the article.";
        let created = mutator
            .create("Article", article("t", first))
            .await
            .unwrap();
        for edit in 0..20 {
            let text = format!("Edit {edit} of the article.");
            assert_eq!(
                update_body(&mutator, &created.doc_id, &text)
                    .await
                    .derived
                    .len(),
                1
            );
        }

        let txn = db::DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
        let back = update_body(&txn, &created.doc_id, first).await;
        assert!(back.derived.is_empty());
        txn.take_txn().await.unwrap().commit().await.unwrap();
        assert_eq!(
            body_of(&mutator, &created.doc_id).await.as_deref(),
            Some(first)
        );
    })
    .unwrap()
}
