//! A document sync of a parent sends only the chunks of its current text.

use db::chunk::split::split;
use db::merge::head_provider::DbHeadProvider;
use document::NormalValue;
use p2p::sync::DocumentHeadProvider;
use query::mutator::{DerivedWrite, DocMutator};
use rapidhash::RapidHashSet;

use super::sync::{article, book, setup, text_of, update_body, CONFIG};

fn hash_of(write: &DerivedWrite) -> String {
    write
        .document
        .as_ref()
        .and_then(|doc| doc.get("hash"))
        .and_then(NormalValue::as_str)
        .expect("chunk hash")
        .to_string()
}

#[test]
fn an_edited_parent_follows_only_its_current_chunks() {
    workers::block_on(async {
        let (mutator, db, _keep) = setup().await;
        let mut paras = book();
        let created = mutator
            .create("Article", article("t", &text_of(&paras)))
            .await
            .unwrap();
        let mut written = created.derived;
        for edit in 0..4 {
            paras[edit * 3] = format!("Edit {edit} rewrites this paragraph.");
            written.extend(
                update_body(&mutator, &created.doc_id, &text_of(&paras))
                    .await
                    .derived,
            );
        }
        let current = text_of(&paras);
        let current_hashes: RapidHashSet<String> = split(&current, &CONFIG)
            .into_iter()
            .map(|chunk| chunk.hash)
            .collect();
        let mut expected: Vec<String> = written
            .iter()
            .filter(|write| current_hashes.contains(&hash_of(write)))
            .map(|write| write.doc_id.to_string())
            .collect();
        expected.sort();
        assert!(written.len() > expected.len(), "the edits left old chunks");

        let provider = DbHeadProvider::new(db);
        let mut followers = provider
            .following_doc_ids(&created.doc_id.to_string())
            .await
            .unwrap();
        followers.sort();
        assert_eq!(followers, expected);
    })
    .unwrap()
}

#[test]
fn a_deleted_parent_follows_nothing() {
    workers::block_on(async {
        let (mutator, db, _keep) = setup().await;
        let created = mutator
            .create("Article", article("t", &text_of(&book())))
            .await
            .unwrap();
        let provider = DbHeadProvider::new(db);
        let parent = created.doc_id.to_string();
        assert_eq!(
            provider.following_doc_ids(&parent).await.unwrap().len(),
            created.derived.len()
        );

        mutator.delete("Article", &created.doc_id).await.unwrap();

        assert!(provider
            .following_doc_ids(&parent)
            .await
            .unwrap()
            .is_empty());
        assert!(provider
            .following_doc_ids("bae-00000000-0000-0000-0000-000000000000")
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap()
}
