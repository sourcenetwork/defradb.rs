//! Reading chunks through GraphQL. A chunk is visible only while its parent's
//! current text uses it.

use std::sync::Arc;

use db::{AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher};
use query::{QueryExecutor, QueryRequest, QueryRunner};
use serde_json::Value;

use crate::common::fixture::make_test_db_with_bus;

type Runner = QueryRunner<
    LensedAutoCommitFetcher<storage::RegolithStore>,
    DbTransactionRegistry<storage::RegolithStore>,
>;

const SDL: &str = r#"
    type Article {
        title: String
        body: String @chunk(maxChars: 64)
    }
"#;

async fn runner() -> (Runner, impl Sized) {
    let (db, bus) = make_test_db_with_bus().await;
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .unwrap();
    let runner = QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(&db)),
        DbCollectionProvider::new_arc(Arc::clone(&db)),
        Arc::new(DbTransactionRegistry::new(Arc::clone(&db))),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&db))));
    (runner, (db, bus))
}

async fn run(runner: &Runner, query: &str) -> Value {
    let response = runner.execute(QueryRequest::new(query)).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("data")
}

fn paragraphs(n: usize, word: &str) -> String {
    (0..n)
        .map(|i| format!("Paragraph {i} is about {word} and nothing else."))
        .collect::<Vec<_>>()
        .join("\\n\\n")
}

async fn create(runner: &Runner, body: &str) -> String {
    let data = run(
        runner,
        &format!(
            r#"mutation {{ add_Article(input: {{title: "t", body: "{body}"}}) {{ _docID }} }}"#
        ),
    )
    .await;
    data["add_Article"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn chunk_hashes(runner: &Runner) -> Vec<String> {
    let data = run(runner, "query { Article__body_chunk { hash } }").await;
    let mut hashes: Vec<String> = data["Article__body_chunk"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["hash"].as_str().unwrap().to_string())
        .collect();
    hashes.sort();
    hashes
}

async fn listed(runner: &Runner, doc_id: &str) -> Vec<String> {
    let data = run(
        runner,
        &format!(r#"query {{ Article(docID: "{doc_id}") {{ _body_chunks }} }}"#),
    )
    .await;
    let mut list: Vec<String> = data["Article"][0]["_body_chunks"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .map(|h| h.as_str().unwrap().to_string())
        .collect();
    list.sort();
    list.dedup();
    list
}

#[test]
fn chunks_are_listed_by_their_parent_and_readable() {
    workers::block_on(async {
        let (runner, _keep) = runner().await;
        let doc_id = create(&runner, &paragraphs(6, "rivers")).await;

        let list = listed(&runner, &doc_id).await;
        assert!(!list.is_empty());
        assert_eq!(chunk_hashes(&runner).await, list);

        let data = run(
            &runner,
            "query { Article__body_chunk { _sourceID source { title } } }",
        )
        .await;
        for chunk in data["Article__body_chunk"].as_array().unwrap() {
            assert_eq!(chunk["_sourceID"], doc_id.as_str());
            assert_eq!(chunk["source"]["title"], "t");
        }
    })
    .unwrap()
}

#[test]
fn superseded_chunks_are_hidden() {
    workers::block_on(async {
        let (runner, _keep) = runner().await;
        let doc_id = create(&runner, &paragraphs(6, "rivers")).await;
        run(
            &runner,
            &format!(
                r#"mutation {{ update_Article(docID: "{doc_id}", input: {{body: "{}"}}) {{ _docID }} }}"#,
                paragraphs(3, "mountains")
            ),
        )
        .await;

        assert_eq!(chunk_hashes(&runner).await, listed(&runner, &doc_id).await);
    })
    .unwrap()
}

#[test]
fn a_deleted_parent_leaves_no_readable_chunk_or_commit() {
    workers::block_on(async {
        let (runner, _keep) = runner().await;
        let doc_id = create(&runner, &paragraphs(4, "rivers")).await;
        let data = run(&runner, "query { Article__body_chunk { _docID } }").await;
        let chunk_id = data["Article__body_chunk"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_string();

        run(
            &runner,
            &format!(r#"mutation {{ delete_Article(docID: "{doc_id}") {{ _docID }} }}"#),
        )
        .await;

        assert!(chunk_hashes(&runner).await.is_empty());
        let commits = run(
            &runner,
            &format!(r#"query {{ _commits(docID: "{chunk_id}") {{ cid }} }}"#),
        )
        .await;
        assert_eq!(commits["_commits"], serde_json::json!([]));
    })
    .unwrap()
}

#[test]
fn generated_mutations_are_refused() {
    workers::block_on(async {
        let (runner, _keep) = runner().await;
        let response = runner
            .execute(QueryRequest::new(
                r#"mutation { add_Article__body_chunk(input: {text: "x"}) { _docID } }"#,
            ))
            .await;
        assert!(response.has_errors());
    })
    .unwrap()
}
