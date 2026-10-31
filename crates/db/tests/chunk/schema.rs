use crate::common::fixture::make_test_db_with_bus;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;

#[test]
fn chunked_schema_is_accepted_with_its_generated_collection() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
            .await
            .expect("schema");
        let article = db.get_collection("Article").unwrap().expect("parent");
        let chunk = db
            .get_collection("Article__body_chunk")
            .unwrap()
            .expect("chunks");
        assert_eq!(article.schema().chunked_fields.len(), 1);
        assert_eq!(
            chunk.schema().chunk_of.as_ref().unwrap().collection,
            article.schema().name
        );
    })
    .unwrap()
}
