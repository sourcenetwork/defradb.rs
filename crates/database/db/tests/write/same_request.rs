//! A mutation that names a document created earlier in the same request, by
//! the ID its create returns, updates it. With a KMS configured every write of
//! the request is prepared before any of it runs, while that document does not
//! exist yet.

use db::{AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher};
use query::{QueryExecutor, QueryRequest, QueryRunner};
use std::sync::Arc;

type Runner = QueryRunner<
    LensedAutoCommitFetcher<storage::RegolithStore>,
    DbTransactionRegistry<storage::RegolithStore>,
>;

/// A node with a KMS and the `Secrets` collection, and a runner over it.
async fn node() -> (super::kms::Fixture, Runner) {
    let fixture = super::kms::fixture().await;
    let db = Arc::clone(&fixture.db);
    let runner = QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(&db)),
        DbCollectionProvider::new_arc(Arc::clone(&db)),
        Arc::new(DbTransactionRegistry::new(Arc::clone(&db))),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(db)));
    (fixture, runner)
}

const CREATE: &str = r#"add_Secrets(input: {left: "l", right: "r", public: "before"}) { _docID }"#;

async fn run(runner: &Runner, query: &str) -> serde_json::Value {
    let response = runner.execute(QueryRequest::new(query)).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("a response without errors has data")
}

#[test]
fn a_same_request_update_by_the_created_document_id_applies_with_kms_on() {
    workers::block_on(async {
        // The ID a create of this content derives, learned on a node of its own:
        // an unencrypted document's ID follows from its content alone.
        let (_first_fixture, first) = node().await;
        let created = run(&first, &format!("mutation {{ {CREATE} }}")).await;
        let doc_id = created["add_Secrets"][0]["_docID"]
            .as_str()
            .expect("the create returns its document ID")
            .to_string();

        let (fixture, second) = node().await;
        assert!(fixture.db.kms().is_some());
        let data = run(
            &second,
            &format!(
                r#"mutation {{
                {CREATE}
                update_Secrets(docID: "{doc_id}", input: {{public: "after"}}) {{ _docID public }}
            }}"#
            ),
        )
        .await;
        assert_eq!(data["add_Secrets"][0]["_docID"], doc_id.as_str());
        assert_eq!(data["update_Secrets"][0]["_docID"], doc_id.as_str());
        assert_eq!(data["update_Secrets"][0]["public"], "after");

        let read = run(&second, "{ Secrets { _docID public } }").await;
        assert_eq!(read["Secrets"][0]["_docID"], doc_id.as_str());
        assert_eq!(read["Secrets"][0]["public"], "after");
    })
    .unwrap()
}
