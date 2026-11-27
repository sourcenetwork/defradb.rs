//! A ranking runs the chunk collection's own app read check, as a chunk query
//! does (`defra.chunk.rank.own_app`).

use std::sync::Arc;

use async_trait::async_trait;
use db::chunk::split::{split, ChunkConfig, SplitterVersion};
use db::LensedAutoCommitFetcher;
use document::NormalValue;
use query::access_hooks::{ReadRequest, ReadValidator};
use query::fetcher::DocFetcher;
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

const CONFIG: ChunkConfig = ChunkConfig {
    version: SplitterVersion::CURRENT,
    max_chars: 64,
    overlap: 0,
};
const FIRST: &str = "The first paragraph tells of rivers and the sea.";
const SECOND: &str = "The second paragraph tells of engines and steam.";
const EDITED: &str = "The second paragraph now tells of music and song.";

fn hashes(paragraphs: &[&str]) -> Vec<String> {
    split(&paragraphs.join("\n\n"), &CONFIG)
        .into_iter()
        .map(|chunk| chunk.hash)
        .collect()
}

#[test]
fn one_index_read_answers_each_parent_and_hash_on_its_own() {
    workers::block_on(async {
        let provider = Provider::start();
        let (db, open) = setup(&provider.sdl("@index(vector: {dimensions: 2})")).await;
        let created = run(
            &open,
            &format!(
                r#"mutation {{ add_Article(input: {{body: "{FIRST}\n\n{SECOND}"}}) {{ _docID }} }}"#
            ),
        )
        .await;
        let id = created["add_Article"][0]["_docID"].as_str().unwrap().to_string();
        run(
            &open,
            &format!(
                r#"mutation {{ update_Article(docID: "{id}", input: {{body: "{FIRST}\n\n{EDITED}"}}) {{ _docID }} }}"#
            ),
        )
        .await;
        let chunks = db.get_collection(CHUNKS).unwrap().unwrap();
        let index = chunks
            .schema()
            .indexes
            .iter()
            .find(|index| {
                let fields: Vec<&str> = index.fields.iter().map(|f| f.name.as_str()).collect();
                fields == ["_sourceID", "hash"]
            })
            .unwrap()
            .name
            .clone();
        let (old, new) = (hashes(&[FIRST, SECOND]), hashes(&[FIRST, EDITED]));
        assert_eq!((old.len(), new.len(), old[0] == new[0]), (2, 2, true));
        let parent = NormalValue::String(id);
        let prefixes = vec![
            vec![parent.clone(), NormalValue::String(new[0].clone())],
            vec![parent.clone(), NormalValue::String(new[1].clone())],
            vec![parent.clone(), NormalValue::String(old[1].clone())],
            vec![parent.clone(), NormalValue::String("no such hash".into())],
            vec![parent],
        ];
        let found = LensedAutoCommitFetcher::new(Arc::clone(&db))
            .index_prefix_entries(CHUNKS, &index, &prefixes)
            .await
            .unwrap()
            .unwrap();
        let counts: Vec<usize> = found.iter().map(Vec::len).collect();
        // The superseded chunk stays stored, so only the parent alone reaches all three.
        assert_eq!(counts, vec![1, 1, 1, 0, 3]);
    })
    .unwrap()
}
