use std::sync::Arc;

use defra_core::RequestContext;
use document::{Document, NormalValue};
use query::documents::Filter;
use query::DocMutator;

use crate::common::fixture::make_test_db_with_bus;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;

fn articles() -> schema::CollectionVersion {
    query::parse_sdl(SDL)
        .unwrap()
        .into_iter()
        .find(|c| c.name == "Article")
        .expect("collection")
}

fn article(title: &str) -> Document {
    let mut doc = Document::new(articles());
    doc.set("title", NormalValue::String(title.into())).unwrap();
    doc.set(
        "body",
        NormalValue::String(format!(
            "First part of {title}.\n\nSecond part of {title}, a little longer than the first."
        )),
    )
    .unwrap();
    doc
}

fn title(value: &str) -> Filter {
    serde_json::json!({"title": {"_eq": value}}).into()
}

#[test]
fn truncating_the_parent_empties_its_chunks_and_chunks_alone_are_refused() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
            .await
            .unwrap();
        let mutator = db::AutoCommitMutator::new(Arc::clone(&db));
        let created = mutator.create("Article", article("a")).await.unwrap();

        let err = db
            .truncate_collection_as(&RequestContext::ambient(), "Article__body_chunk", None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("emptied only with its parent"),
            "{err}"
        );

        db.truncate_collection_as(&RequestContext::ambient(), "Article", None)
            .await
            .unwrap();
        for write in &created.derived {
            assert!(!mutator
                .exists("Article__body_chunk", &write.doc_id)
                .await
                .unwrap());
        }
    })
    .unwrap()
}

#[test]
fn a_filtered_truncate_takes_only_the_matching_parents_chunks() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
            .await
            .unwrap();
        let mutator = db::AutoCommitMutator::new(Arc::clone(&db));
        let gone = mutator.create("Article", article("a")).await.unwrap();
        let kept = mutator.create("Article", article("b")).await.unwrap();

        db.truncate_collection_as(&RequestContext::ambient(), "Article", Some(&title("a")))
            .await
            .unwrap();

        for write in &gone.derived {
            assert!(!mutator
                .exists("Article__body_chunk", &write.doc_id)
                .await
                .unwrap());
        }
        for write in &kept.derived {
            assert!(mutator
                .exists("Article__body_chunk", &write.doc_id)
                .await
                .unwrap());
        }
    })
    .unwrap()
}

#[test]
fn a_chunk_collection_is_deleted_only_with_its_parent() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
            .await
            .unwrap();

        let err = db
            .delete_collection("Article__body_chunk")
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("deleted only with its parent"),
            "{err}"
        );
        let err = db
            .delete_collections(vec!["Article__body_chunk".into()], true)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("deleted only with its parent"),
            "{err}"
        );
        assert!(db.get_collection("Article__body_chunk").unwrap().is_some());
    })
    .unwrap()
}

#[test]
fn deleting_the_parent_deletes_its_chunk_collection() {
    workers::block_on(async {
        for batch in [false, true] {
            let (db, _bus) = make_test_db_with_bus().await;
            db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
                .await
                .unwrap();

            if batch {
                db.delete_collections(vec!["Article".into()], false)
                    .await
                    .unwrap();
            } else {
                db.delete_collection("Article").await.unwrap();
            }

            assert!(db.get_collection("Article__body_chunk").unwrap().is_none());
            // So the same schema can be added again.
            db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
                .await
                .unwrap();
            assert!(db.get_collection("Article__body_chunk").unwrap().is_some());
        }
    })
    .unwrap()
}

#[test]
fn purging_a_parent_purges_its_chunks_and_chunks_alone_are_refused() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
            .await
            .unwrap();
        let mutator = db::AutoCommitMutator::new(Arc::clone(&db));
        let gone = mutator.create("Article", article("a")).await.unwrap();
        let kept = mutator.create("Article", article("b")).await.unwrap();

        let chunk = kept.derived[0].doc_id.clone();
        let err = db
            .purge_by_doc_ids("Article__body_chunk", &[chunk], None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("emptied only with its parent"),
            "{err}"
        );

        let parent = gone.doc_id.clone();
        let purged = db
            .purge_by_doc_ids("Article", &[parent], None)
            .await
            .unwrap();
        assert_eq!(purged, 1);
        for write in &gone.derived {
            assert!(!mutator
                .exists("Article__body_chunk", &write.doc_id)
                .await
                .unwrap());
        }
        for write in &kept.derived {
            assert!(mutator
                .exists("Article__body_chunk", &write.doc_id)
                .await
                .unwrap());
        }
    })
    .unwrap()
}
