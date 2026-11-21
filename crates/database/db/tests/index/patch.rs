use std::sync::Arc;

use defra_core::RequestContext;
use query::{QueryExecutor, QueryRequest};
use storage::RegolithStore;

use db::{DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB};

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

        let users = db.get_collection("Users").unwrap().unwrap();
        assert!(users
            .schema()
            .fields
            .iter()
            .any(|field| field.name == "name"));
        assert_eq!(users.schema().indexes.len(), 1);

        let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&db)));
        let queries: Arc<dyn QueryExecutor> = Arc::new(
            query::QueryRunner::with_arc_registry_and_provider(
                LensedAutoCommitFetcher::new(Arc::clone(&db)),
                DbCollectionProvider::new_arc(Arc::clone(&db)),
                registry,
            )
            .with_mutator(Arc::new(db::AutoCommitMutator::new(Arc::clone(&db)))),
        );
        let response = queries
            .execute(QueryRequest::new(
                r#"mutation { add_Users(input: {name: "ann"}) { _docID } }"#,
            ))
            .await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
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
