use db::chunk::access::parent_collection;

use crate::common::fixture::make_test_db_with_bus;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
    type Plain {
        body: String
    }
"#;

#[test]
fn a_chunk_collections_parent_must_chunk_the_named_field() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.add_collection(&defra_core::RequestContext::ambient(), SDL)
            .await
            .expect("schema");
        let chunks_col = db
            .get_collection("Article__body_chunk")
            .unwrap()
            .unwrap()
            .schema()
            .clone();

        let parent = parent_collection(&db, &chunks_col)
            .unwrap()
            .expect("parent");
        assert_eq!(parent.name(), "Article");

        let mut unchunked_parent = chunks_col.clone();
        if let Some(source) = unchunked_parent.chunk_of.as_mut() {
            source.collection = "Plain".into();
        }
        assert!(parent_collection(&db, &unchunked_parent).unwrap().is_none());

        let mut unchunked_field = chunks_col.clone();
        if let Some(source) = unchunked_field.chunk_of.as_mut() {
            source.field_name = "title".into();
        }
        assert!(parent_collection(&db, &unchunked_field).unwrap().is_none());

        let mut missing_parent = chunks_col.clone();
        if let Some(source) = missing_parent.chunk_of.as_mut() {
            source.collection = "Gone".into();
        }
        assert!(parent_collection(&db, &missing_parent).unwrap().is_none());

        let articles = db.get_collection("Article").unwrap().unwrap();
        assert!(parent_collection(&db, articles.schema()).unwrap().is_none());
    })
    .unwrap()
}
