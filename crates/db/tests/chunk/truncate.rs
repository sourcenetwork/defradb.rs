use std::sync::Arc;

use document::{Document, NormalValue};
use query::{DocMutator, Filter};

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
    let mut conditions = serde_json::Map::new();
    conditions.insert("title".into(), serde_json::json!({"_eq": value}));
    Filter::from_conditions(conditions)
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
            .truncate_collection("Article__body_chunk", None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("emptied only with its parent"),
            "{err}"
        );

        db.truncate_collection("Article", None).await.unwrap();
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

        db.truncate_collection_with_filter("Article", title("a"), None)
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
