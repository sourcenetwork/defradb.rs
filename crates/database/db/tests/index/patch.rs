use defra_core::RequestContext;
use storage::RegolithStore;

use db::DB;

#[test]
fn removing_an_indexed_field_is_refused_with_the_index_name() {
    workers::block_on(async {
        let store = std::sync::Arc::new(RegolithStore::in_memory().unwrap());
        let db = std::sync::Arc::new(DB::from_arc(store).unwrap());
        db.add_collection(
            &RequestContext::default(),
            r#"type Users { name: String @index age: Int }"#,
        )
        .await
        .unwrap();

        let error = db
            .patch_collection(
                &RequestContext::default(),
                r#"[{"op":"remove","path":"/Users/Fields/name"}]"#,
                None,
            )
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("index 'Users_name_ASC' references field 'name'"),
            "unexpected error: {error}"
        );
    })
    .unwrap()
}

#[test]
fn removing_a_fulltext_field_is_refused_with_the_index_name() {
    workers::block_on(async {
        let store = std::sync::Arc::new(RegolithStore::in_memory().unwrap());
        let db = std::sync::Arc::new(DB::from_arc(store).unwrap());
        db.add_collection(
            &RequestContext::default(),
            r#"type Notes { body: String @fulltext }"#,
        )
        .await
        .unwrap();

        let error = db
            .patch_collection(
                &RequestContext::default(),
                r#"[{"op":"remove","path":"/Notes/Fields/body"}]"#,
                None,
            )
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("full-text index 'body' references field 'body'"),
            "unexpected error: {error}"
        );
    })
    .unwrap()
}
