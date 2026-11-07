//! The unrouted chunked `SIMILARITY`: each parent scored from its listed chunks.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use rapidhash::RapidHashSet;

use super::listed::{best_listed_score, best_score_of};
use super::target::{ChunkTarget, Reads};
use crate::document::DocumentMapping;
use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::plan::chunk_gate::get_projected;
use crate::plan::chunk_parent::{listed_hashes, parent_fields};
use crate::planner::{index_selection::CursorSeek, Doc, ExecInfo, PlanNode};

/// Scores each parent row by its best listed chunk, as `SimilarityNode` scores its own vector.
pub struct ChunkSimilarityNode {
    source: Box<dyn PlanNode>,
    mapping: DocumentMapping,
    similarity_index: usize,
    target: ChunkTarget,
    fetcher: Option<Arc<dyn DocFetcher>>,
    in_row: Option<ListedInRow>,
    parent_fields: RapidHashSet<String>,
    current: Doc,
    exec_info: ExecInfo,
}

/// Where a parent row holds its chunked text and its chunk list.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ListedInRow {
    pub text: usize,
    pub list: usize,
}

/// The chunk hashes a parent row's text lists, or `None` when it has no text.
fn listed_in_row(row: &Doc, at: ListedInRow, target: &ChunkTarget) -> Option<RapidHashSet<String>> {
    let text = row.get(at.text)?.as_str()?;
    let list: Option<Vec<String>> = row.get(at.list).and_then(JsonValue::as_array).map(|list| {
        list.iter()
            .filter_map(|hash| hash.as_str().map(str::to_string))
            .collect()
    });
    Some(listed_hashes(text, list.as_deref(), &target.chunked))
}

impl ChunkSimilarityNode {
    pub(crate) fn new(
        source: Box<dyn PlanNode>,
        mapping: DocumentMapping,
        similarity_index: usize,
        target: ChunkTarget,
        fetcher: Option<Arc<dyn DocFetcher>>,
        in_row: Option<ListedInRow>,
    ) -> Self {
        Self {
            source,
            mapping,
            similarity_index,
            parent_fields: parent_fields(&target.chunked),
            target,
            fetcher,
            in_row,
            current: Doc::default(),
            exec_info: ExecInfo::default(),
        }
    }
}

/// A parent's best listed chunk's score, from its row when the row holds its list.
async fn score_parent(
    node: &ScoreInputs<'_>,
    row: &Doc,
    parent_id: &str,
    reads: &mut Reads,
) -> Result<Option<f64>> {
    let Some(fetcher) = node.fetcher else {
        return Ok(None);
    };
    if let Some(at) = node.in_row {
        let Some(listed) = listed_in_row(row, at, node.target) else {
            return Ok(None);
        };
        return best_score_of(fetcher, node.target, parent_id, &listed, reads).await;
    }
    let Some(parent) =
        get_projected(fetcher, &node.target.parent, parent_id, node.parent_fields).await?
    else {
        return Ok(None);
    };
    best_listed_score(fetcher, node.target, &parent, reads).await
}

/// The parts of the node scoring reads, borrowed apart from its source.
struct ScoreInputs<'a> {
    fetcher: Option<&'a dyn DocFetcher>,
    target: &'a ChunkTarget,
    in_row: Option<ListedInRow>,
    parent_fields: &'a RapidHashSet<String>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PlanNode for ChunkSimilarityNode {
    async fn init(&mut self) -> Result<()> {
        self.exec_info = ExecInfo::default();
        self.source.init().await
    }

    async fn start(&mut self) -> Result<()> {
        self.source.start().await
    }

    async fn next(&mut self) -> Result<bool> {
        self.exec_info.iterations += 1;
        if !self.source.next().await? {
            return Ok(false);
        }
        let mut row = self.source.value().deep_clone();
        let inputs = ScoreInputs {
            fetcher: self.fetcher.as_deref(),
            target: &self.target,
            in_row: self.in_row,
            parent_fields: &self.parent_fields,
        };
        let mut reads = Reads::default();
        let score = match row.doc_id() {
            Some(id) => score_parent(&inputs, &row, id, &mut reads).await?,
            None => None,
        };
        self.exec_info.docs_fetched += reads.docs;
        self.exec_info.fields_fetched += reads.fields;
        let score = score
            .and_then(serde_json::Number::from_f64)
            .map_or(JsonValue::Null, JsonValue::Number);
        row.set(self.similarity_index, score);
        self.current = row;
        Ok(true)
    }

    fn value(&self) -> &Doc {
        &self.current
    }

    async fn close(&mut self) -> Result<()> {
        self.source.close().await
    }

    fn source(&self) -> Option<&dyn PlanNode> {
        Some(self.source.as_ref())
    }

    fn document_map(&self) -> &DocumentMapping {
        &self.mapping
    }

    fn set_cursor_seek(&mut self, seek: CursorSeek) -> bool {
        self.source.set_cursor_seek(seek)
    }

    fn set_vector_parent(&mut self, field_index: usize, doc_id: &str) -> bool {
        self.source.set_vector_parent(field_index, doc_id)
    }

    fn page_info(&self) -> Option<crate::plan::CursorPageInfo> {
        self.source.page_info()
    }

    fn set_cursor_fetch_limit(&mut self, _limit: u64) -> bool {
        // Scoring consumes all input to rank it; bounding the scan below would drop candidates.
        false
    }

    fn kind(&self) -> &'static str {
        "similarityNode"
    }

    fn exec_info(&self) -> ExecInfo {
        self.exec_info.clone()
    }

    fn explain_execute_inner(&self) -> JsonValue {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "iterations".to_string(),
            serde_json::json!(self.exec_info.iterations),
        );
        if let Some(child) = self.source.explain_execute().as_object() {
            obj.extend(child.clone());
        }
        JsonValue::Object(obj)
    }
}
