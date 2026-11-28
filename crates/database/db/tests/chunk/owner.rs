//! A chunk error names the parent document and field it was for, and keeps its kind.

use db::chunk::ChunkOwner;
use query::error::QueryError;

const SDL: &str = "type Article { title: String body: String @chunk(maxChars: 64) }";
const PARENT: &str = "bae-0f7e43b1-6b0b-5bd5-9b6b-6c5a1f4c2d11";

fn owner() -> ChunkOwner<'static> {
    ChunkOwner {
        collection: "Article",
        field: "body",
        doc_id: PARENT,
    }
}

#[test]
fn a_chunk_collection_names_its_parent_field() {
    workers::block_on(async {
        let (db, _bus) = crate::common::fixture::make_test_db_with_bus().await;
        db.add_collection(&defra_core::RequestContext::ambient(), SDL)
            .await
            .unwrap();
        let chunks = db
            .get_all_collection_versions()
            .await
            .unwrap()
            .into_iter()
            .find(|version| version.name == "Article__body_chunk")
            .expect("the chunk collection");

        assert_eq!(ChunkOwner::of(&chunks, PARENT), owner());
    })
    .unwrap()
}

#[test]
fn an_execution_error_names_the_parent_document_and_field() {
    let error = owner().name_in(QueryError::execution("chunk scan failed: closed"));

    assert!(matches!(error, QueryError::Execution(_)), "{error:?}");
    assert_eq!(
        error.to_string(),
        format!("chunks of field Article.body of document {PARENT}: chunk scan failed: closed")
    );
}

#[test]
fn an_internal_error_names_the_parent_document_and_field() {
    let error = owner().name_in(QueryError::Internal(
        "invalid prepared chunk short ID".into(),
    ));

    assert_eq!(
        error.to_string(),
        format!(
            "internal error: chunks of field Article.body of document {PARENT}: invalid prepared chunk short ID"
        )
    );
}

#[test]
fn a_conflict_keeps_its_kind_and_message() {
    let error = owner().name_in(QueryError::TransactionConflict("write conflict".into()));

    assert!(
        matches!(&error, QueryError::TransactionConflict(message) if message == "write conflict"),
        "{error:?}"
    );
}
