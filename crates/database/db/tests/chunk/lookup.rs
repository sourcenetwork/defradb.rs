//! A write reuses only its own parent's chunks.

use document::NormalValue;
use query::mutator::DocMutator;

use super::sync::{article, book, setup, update_body, CHUNKS};

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
