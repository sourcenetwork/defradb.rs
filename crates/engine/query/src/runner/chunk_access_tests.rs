use async_trait::async_trait;
use chunk_split::split::{split, ChunkConfig};
use document::{DocID, Document, NormalValue};
use schema::CollectionVersion;

use crate::doc_stream::DocStream;
use crate::error::Result;
use crate::fetcher::{
    DocFetcher, DocumentArrival, DocumentArrivalOptions, DocumentArrivalPage, FetchByIdsResult,
};
use crate::test_utils::MockFetcher;
use crate::{QueryExecutor, QueryRequest, QueryRunner};

const CHUNKS: &str = "Article__body_chunk";
const BODY: &str = "Some text.";

/// Documents in memory, with every stored chunk on the arrival journal.
struct Store {
    docs: MockFetcher,
    chunk_ids: Vec<String>,
}

#[async_trait]
impl DocFetcher for Store {
    async fn get_all(&self, collection_name: &str) -> Result<Vec<Document>> {
        self.docs.get_all(collection_name).await
    }

    async fn stream_by_doc_short_ids(
        &self,
        collection_name: &str,
        doc_short_ids: &[u64],
        show_deleted: bool,
    ) -> Result<Box<dyn DocStream>> {
        self.docs
            .stream_by_doc_short_ids(collection_name, doc_short_ids, show_deleted)
            .await
    }

    async fn stream_all_with_deleted(
        &self,
        collection_name: &str,
        show_deleted: bool,
    ) -> Result<Box<dyn DocStream>> {
        self.docs
            .stream_all_with_deleted(collection_name, show_deleted)
            .await
    }

    async fn get_by_ids(
        &self,
        collection_name: &str,
        doc_ids: &[String],
    ) -> Result<FetchByIdsResult> {
        self.docs.get_by_ids(collection_name, doc_ids).await
    }

    async fn get_by_field_value(
        &self,
        collection_name: &str,
        field_name: &str,
        value: &str,
    ) -> Result<Vec<Document>> {
        self.docs
            .get_by_field_value(collection_name, field_name, value)
            .await
    }

    async fn get_document_arrivals(
        &self,
        _options: &DocumentArrivalOptions,
    ) -> Result<DocumentArrivalPage> {
        let entries: Vec<DocumentArrival> = self
            .chunk_ids
            .iter()
            .enumerate()
            .map(|(cursor, doc_id)| DocumentArrival {
                cursor: cursor as u64 + 1,
                doc_id: doc_id.clone(),
            })
            .collect();
        let head = entries.len() as u64;
        Ok(DocumentArrivalPage {
            head,
            next: head,
            entries,
        })
    }
}

fn collections() -> Vec<CollectionVersion> {
    crate::parse_sdl("type Article { body: String @chunk(maxChars: 64) }").unwrap()
}

/// A live article with one chunk of its text, read with `known` collections.
fn runner(known: Vec<CollectionVersion>) -> (QueryRunner<Store>, String) {
    let all = collections();
    let (articles, chunks) = (&all[0], &all[1]);
    let chunked = &articles.chunked_fields[0];
    let config = ChunkConfig::new(
        chunked.splitter_version,
        chunked.max_chars as usize,
        chunked.overlap as usize,
    )
    .unwrap();
    let piece = split(BODY, &config).remove(0);

    let docs = MockFetcher::new();
    let parent_id = DocID::new_v0_from_seed("parent");
    let mut parent = Document::new(articles.clone());
    parent.set_id(parent_id.clone());
    parent
        .set("body", NormalValue::String(BODY.into()))
        .unwrap();
    docs.add_doc(&articles.name, parent);
    let chunk_id = DocID::new_v0_from_seed("chunk");
    let mut chunk = Document::new(chunks.clone());
    chunk.set_id(chunk_id.clone());
    for (field, value) in [
        ("_sourceID", parent_id.to_string()),
        ("text", piece.text),
        ("hash", piece.hash),
    ] {
        chunk.set(field, NormalValue::String(value)).unwrap();
    }
    docs.add_doc(CHUNKS, chunk);

    let store = Store {
        docs,
        chunk_ids: vec![chunk_id.to_string()],
    };
    (QueryRunner::new(store, known), chunk_id.to_string())
}

async fn shown(runner: &QueryRunner<Store>) -> (serde_json::Value, serde_json::Value) {
    let mut found = Vec::new();
    for query in [
        format!("{{ {CHUNKS} {{ _docID }} }}"),
        format!(r#"{{ _documentArrivals(collection: "{CHUNKS}") {{ entries {{ docID }} }} }}"#),
    ] {
        let response = runner.execute(QueryRequest::new(&query)).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        found.push(response.data.unwrap());
    }
    let arrivals = found.pop().unwrap()["_documentArrivals"]["entries"].clone();
    let listed = found.pop().unwrap()[CHUNKS].clone();
    (listed, arrivals)
}

#[test]
fn a_chunk_shows_when_its_parent_collection_resolves() {
    workers::block_on(async {
        let (runner, chunk_id) = runner(collections());
        let (listed, arrivals) = shown(&runner).await;
        assert_eq!(listed, serde_json::json!([{ "_docID": chunk_id }]));
        assert_eq!(arrivals, serde_json::json!([{ "docID": chunk_id }]));
    })
    .unwrap()
}

#[test]
fn a_chunk_whose_parent_collection_does_not_resolve_is_hidden() {
    workers::block_on(async {
        let chunks_only = collections().into_iter().skip(1).collect();
        let (runner, _) = runner(chunks_only);
        let (listed, arrivals) = shown(&runner).await;
        assert_eq!(listed, serde_json::json!([]));
        assert_eq!(arrivals, serde_json::json!([]));
    })
    .unwrap()
}
