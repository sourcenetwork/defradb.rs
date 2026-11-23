//! `DB::patch_collection` refuses to remove a field an index reads, and the
//! collection keeps the field, the index and its writes.

use std::sync::Arc;

use defra_core::RequestContext;
use query::{QueryExecutor, QueryRequest};
use storage::RegolithStore;

use db::{
    AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB,
};

async fn open(sdl: &str) -> Arc<DB<RegolithStore>> {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store).unwrap());
    db.add_collection(&RequestContext::default(), sdl)
        .await
        .unwrap();
    db
}

async fn refused_patch(db: &DB<RegolithStore>, patch: &str) -> String {
    db.patch_collection(&RequestContext::default(), patch, None)
        .await
        .unwrap_err()
        .to_string()
}

async fn create(db: &Arc<DB<RegolithStore>>, mutation: &str) {
    let queries = query::QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(db)),
        DbCollectionProvider::new_arc(Arc::clone(db)),
        Arc::new(DbTransactionRegistry::new(Arc::clone(db))),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(db))));
    let response = queries.execute(QueryRequest::new(mutation)).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
}

#[test]
fn removing_an_indexed_field_is_refused_with_the_index_name() {
    workers::block_on(async {
        let db = open("type Users { name: String @index age: Int }").await;

        let error = refused_patch(&db, r#"[{"op":"remove","path":"/Users/Fields/name"}]"#).await;
        assert!(
            error.contains(
                "cannot remove field 'name': index 'Users_name_ASC' depends on it, drop the index first. Collection: Users"
            ),
            "unexpected error: {error}"
        );

        let users = db.get_collection("Users").unwrap().unwrap();
        assert!(users.schema().fields.iter().any(|field| field.name == "name"));
        assert_eq!(users.schema().indexes.len(), 1);
        create(&db, r#"mutation { add_Users(input: {name: "ann"}) { _docID } }"#).await;
    })
    .unwrap()
}

#[test]
fn removing_a_fulltext_field_is_refused_with_the_index_name() {
    workers::block_on(async {
        let db = open("type Notes { body: String @fulltext }").await;

        let error = refused_patch(&db, r#"[{"op":"remove","path":"/Notes/Fields/body"}]"#).await;
        assert!(
            error.contains(
                "cannot remove field 'body': a full-text index depends on it, drop the index first. Collection: Notes"
            ),
            "unexpected error: {error}"
        );

        let notes = db.get_collection("Notes").unwrap().unwrap();
        assert!(notes.schema().fields.iter().any(|field| field.name == "body"));
        assert_eq!(notes.schema().fulltext_indexes.len(), 1);
        create(&db, r#"mutation { add_Notes(input: {body: "still writable"}) { _docID } }"#).await;
    })
    .unwrap()
}
