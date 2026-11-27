//! A create whose DocID is stored is refused with a typed error, and that
//! error reaches the GraphQL response's code unwrapped
//! (`defra.document.id.collision_code`).

use db::{AutoCommitMutator, DB};
use document::Document;
use query::error::QueryError;
use query::executor::{
    QueryResponseError, DOCUMENT_ALREADY_EXISTS_ERROR_CODE, DOCUMENT_DELETED_ERROR_CODE,
};
use query::mutator::DocMutator;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use std::sync::Arc;
use storage::RegolithStore;

const DOC: &str = r#"{"name": "same"}"#;

fn collection() -> CollectionVersion {
    CollectionVersion::new(
        "Twin",
        "twin-v1",
        "col-twin",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
}

async fn mutator() -> AutoCommitMutator<RegolithStore> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).expect("create db"));
    db.add_collection(
        &defra_core::RequestContext::ambient(),
        "type Twin { name: String }",
    )
    .await
    .expect("create collection");
    AutoCommitMutator::new(db)
}

fn doc() -> Document {
    Document::from_json_str(DOC, collection()).unwrap()
}

#[test]
fn a_second_create_is_refused_as_existing_with_its_code() {
    workers::block_on(async {
        let mutator = mutator().await;
        let created = mutator.create("Twin", doc()).await.expect("first create");
        let error = mutator
            .create("Twin", doc())
            .await
            .expect_err("the same DocID again is refused");
        let id = created.doc_id.to_string();
        assert!(
            matches!(&error, QueryError::DocumentAlreadyExists(named) if *named == id),
            "{error:?}"
        );
        let response = QueryResponseError::from_query_error(error);
        assert_eq!(
            response.message,
            format!("a document with the given ID already exists. DocID: {id}")
        );
        assert_eq!(response.code(), Some(DOCUMENT_ALREADY_EXISTS_ERROR_CODE));
    })
    .unwrap()
}

#[test]
fn a_create_of_a_deleted_document_is_refused_as_deleted_with_its_code() {
    workers::block_on(async {
        let mutator = mutator().await;
        let created = mutator.create("Twin", doc()).await.expect("create");
        mutator
            .delete("Twin", &created.doc_id)
            .await
            .expect("delete");
        let error = mutator
            .create("Twin", doc())
            .await
            .expect_err("a deleted DocID is refused");
        let id = created.doc_id.to_string();
        assert!(
            matches!(&error, QueryError::DocumentDeleted(named) if *named == id),
            "{error:?}"
        );
        let response = QueryResponseError::from_query_error(error);
        assert_eq!(
            response.message,
            format!("a document with the given ID has been deleted. DocID: {id}")
        );
        assert_eq!(response.code(), Some(DOCUMENT_DELETED_ERROR_CODE));
    })
    .unwrap()
}
