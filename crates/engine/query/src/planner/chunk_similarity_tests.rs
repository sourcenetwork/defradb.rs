//! An app read validator keeps a chunked similarity off the chunk index, as ACP does.

use std::sync::Arc;

use async_trait::async_trait;
use document::Document;
use schema::CollectionVersion;

use crate::access_hooks::{ReadRequest, ReadValidator};
use crate::doc_stream::DocStream;
use crate::error::Result;
use crate::fetcher::{DocFetcher, FetchByIdsResult};
use crate::planner::Planner;

const SDL: &str = r#"type Article {
    body: String @chunk(maxChars: 64)
    body_v: [Float32!] @embedding(fields: ["body"], provider: "openai", model: "m") @index(vector: {dimensions: 3})
}"#;

const RANKING: &str = r#"query { Article(order: {_alias: {sim: DESC}}, limit: 2) {
    sim: SIMILARITY(body_v: {vector: [1, 0, 0]})
} }"#;

/// A fetcher that can search vectors; planning never reads through it.
struct VectorFetcher;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocFetcher for VectorFetcher {
    async fn get_all(&self, _: &str) -> Result<Vec<Document>> {
        unreachable!("planning reads nothing")
    }

    async fn stream_by_doc_short_ids(
        &self,
        _: &str,
        _: &[u64],
        _: bool,
    ) -> Result<Box<dyn DocStream>> {
        unreachable!("planning reads nothing")
    }

    async fn stream_all_with_deleted(&self, _: &str, _: bool) -> Result<Box<dyn DocStream>> {
        unreachable!("planning reads nothing")
    }

    async fn get_by_ids(&self, _: &str, _: &[String]) -> Result<FetchByIdsResult> {
        unreachable!("planning reads nothing")
    }

    async fn get_by_field_value(&self, _: &str, _: &str, _: &str) -> Result<Vec<Document>> {
        unreachable!("planning reads nothing")
    }

    fn supports_vector_search(&self) -> bool {
        true
    }
}

/// An app read validator that governs one collection.
struct Governs(&'static str);

#[async_trait]
impl ReadValidator for Governs {
    fn governs(&self, collection: &CollectionVersion) -> bool {
        collection.name == self.0
    }

    async fn may_read(&self, _: &ReadRequest<'_>) -> std::result::Result<bool, String> {
        Ok(true)
    }
}

/// The `VECTOR_INDEX_UNUSED` reasons planning the ranking gives.
fn unused_reasons(planner: Planner) -> Vec<String> {
    let select = &crate::parse_query(RANKING).unwrap()[0];
    planner
        .plan_with_index_info(select)
        .unwrap()
        .warnings
        .iter()
        .filter_map(|warning| {
            warning
                .detail
                .as_ref()?
                .get("reason")?
                .as_str()
                .map(String::from)
        })
        .collect()
}

/// The node kinds of the plan for `query`, from the top down.
fn plan_kinds(planner: Planner, query: &str) -> Vec<&'static str> {
    let select = &crate::parse_query(query).unwrap()[0];
    let result = planner.plan_with_index_info(select).unwrap();
    let mut kinds = Vec::new();
    let mut node: Option<&dyn crate::planner::PlanNode> = Some(result.plan.as_ref());
    while let Some(current) = node {
        kinds.push(current.kind());
        node = current.source();
    }
    kinds
}

fn planner() -> Planner {
    Planner::new(crate::parse_sdl(SDL).unwrap()).with_fetcher(Arc::new(VectorFetcher))
}

#[test]
fn a_ranking_without_read_rules_routes_through_the_chunk_index() {
    assert!(unused_reasons(planner()).is_empty());
    assert!(!plan_kinds(planner(), RANKING).contains(&"similarityNode"));
}

#[test]
fn a_ranking_narrowed_by_doc_ids_scores_the_chunks_of_its_documents() {
    let query = r#"query { Article(docID: "bae-1", order: {_alias: {sim: DESC}}, limit: 2) {
        sim: SIMILARITY(body_v: {vector: [1, 0, 0]})
    } }"#;
    assert!(plan_kinds(planner(), query).contains(&"similarityNode"));
}

#[test]
fn an_app_read_validator_keeps_the_ranking_off_the_chunk_index() {
    let governed = || planner().with_read_validator(Some(Arc::new(Governs("Article"))));
    assert_eq!(unused_reasons(governed()), vec!["filter".to_string()]);
    assert!(plan_kinds(governed(), RANKING).contains(&"similarityNode"));
}

#[test]
fn the_chunk_collections_own_app_validator_keeps_the_ranking_off_the_chunk_index() {
    let planner = planner().with_read_validator(Some(Arc::new(Governs("Article__body_chunk"))));
    assert_eq!(unused_reasons(planner), vec!["filter".to_string()]);
}
