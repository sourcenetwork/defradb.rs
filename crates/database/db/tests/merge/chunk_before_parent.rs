//! A peer can receive an article's chunks before the article itself.

use std::sync::Arc;

use document::{Document, NormalValue};
use query::mutator::DocMutator;

use super::chunk_forged::{chunks_of, list_of, merge, node, Node};

fn article(node: &Node, body: &str) -> Document {
    let collection = node.db.get_collection("Article").unwrap().unwrap();
    let mut doc = Document::new(collection.schema_arc());
    doc.set("title", NormalValue::String("t".into())).unwrap();
    doc.set("body", NormalValue::String(body.into())).unwrap();
    doc
}

#[test]
fn creating_an_article_whose_chunks_already_arrived_succeeds() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let body = "First paragraph of the story.\n\nSecond paragraph of the story.";

        let created = db::AutoCommitMutator::new(Arc::clone(&author.db))
            .create("Article", article(&author, body))
            .await
            .unwrap();
        assert!(!created.derived.is_empty());
        let collection = chunks_of(&author);
        for chunk in &created.derived {
            let outcome = merge(
                &author,
                &peer,
                &collection,
                &chunk.commit_cid,
                &chunk.doc_id.to_string(),
            )
            .await
            .unwrap();
            assert!(outcome.is_merged(), "{outcome:?}");
        }

        let local = db::AutoCommitMutator::new(Arc::clone(&peer.db))
            .create("Article", article(&peer, body))
            .await
            .expect("the same article is created where its chunks already are");
        assert_eq!(local.doc_id, created.doc_id);
        assert!(local.derived.is_empty());
        assert_eq!(
            list_of(&peer, &local.doc_id).await,
            list_of(&author, &created.doc_id).await
        );
    })
    .unwrap()
}
