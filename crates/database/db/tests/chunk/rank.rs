//! A ranking runs the chunk collection's own app read check, as a chunk query
//! does (`defra.chunk.rank.own_app`).

use std::sync::Arc;

use async_trait::async_trait;
use query::access_hooks::{ReadRequest, ReadValidator};
use query::{QueryExecutor, QueryRequest};
use schema::CollectionVersion;
use serde_json::{json, Value};

use super::embedding::{run, runner, setup, Provider, Runner};

const CHUNKS: &str = "Article__body_chunk";

/// An app read validator that governs only the chunk collection and refuses every read.
struct RefuseChunks;

#[async_trait]
impl ReadValidator for RefuseChunks {
    fn governs(&self, collection: &CollectionVersion) -> bool {
        collection.name == CHUNKS
    }

    async fn may_read(&self, _request: &ReadRequest<'_>) -> Result<bool, String> {
        Ok(false)
    }
}

const RANKED: &str = "query { Article(order: {_alias: {sim: DESC}}, limit: 1) {
    sim: SIMILARITY(body_v: {vector: [1, 0]})
} }";

async fn ranked(runner: &Runner) -> (Value, Vec<String>) {
    let response = runner.execute(QueryRequest::new(RANKED)).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let reasons = response
        .extensions
        .iter()
        .flat_map(|extensions| &extensions.warnings)
        .filter_map(|warning| warning.detail.as_ref()?.get("reason")?.as_str())
        .map(String::from)
        .collect();
    (response.data.expect("data")["Article"].clone(), reasons)
}

#[test]
#[mantra_macros::req_test("defra.chunk.rank.own_app")]
fn a_chunk_its_own_app_check_refuses_never_scores_its_article() {
    workers::block_on(async {
        let provider = Provider::start();
        let (db, open) = setup(&provider.sdl("@index(vector: {dimensions: 2})")).await;
        run(
            &open,
            r#"mutation { add_Article(input: {body: "Rivers run."}) { _docID } }"#,
        )
        .await;
        let (articles, reasons) = ranked(&open).await;
        assert!(articles[0]["sim"].is_number(), "{articles}");
        assert!(reasons.is_empty(), "{reasons:?}");

        let refused = runner(&db).with_read_validator(Arc::new(RefuseChunks));
        let chunks = run(&refused, &format!("{{ {CHUNKS} {{ _docID }} }}")).await;
        assert_eq!(chunks[CHUNKS], json!([]));
        let (articles, reasons) = ranked(&refused).await;
        assert_eq!(articles, json!([{ "sim": null }]));
        assert_eq!(reasons, vec!["filter".to_string()]);
    })
    .unwrap()
}
