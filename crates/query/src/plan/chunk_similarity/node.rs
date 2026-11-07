//! The unrouted chunked `SIMILARITY`: each parent scored from its listed chunks.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};

use super::listed::best_score_of;
use super::target::{ChunkTarget, Reads};
use crate::document::DocumentMapping;
use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::plan::chunk_parent::listed_hashes;
use crate::planner::{index_selection::CursorSeek, Doc, ExecInfo, PlanNode};

/// Scores each parent row by its best listed chunk, as `SimilarityNode` scores its own vector.
pub struct ChunkSimilarityNode {
    source: Box<dyn PlanNode>,
    mapping: DocumentMapping,
    similarity_index: usize,
    target: ChunkTarget,
    fetcher: Option<Arc<dyn DocFetcher>>,
    in_row: ListedInRow,
    current: Doc,
    exec_info: ExecInfo,
}

/// Where a parent row holds its chunked text and its chunk list.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ListedInRow {
    pub text: usize,
    pub list: usize,
}

/// The chunk hashes a live parent row's text lists, or `None` when it has no text.
pub(crate) fn listed_in_row(
    row: &Doc,
    at: ListedInRow,
    target: &ChunkTarget,
) -> Option<RapidHashSet<String>> {
    // A deleted parent shows no chunks, even ones a merge has not removed yet.
    if row.is_deleted() {
        return None;
    }
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
        in_row: ListedInRow,
    ) -> Self {
        Self {
            source,
            mapping,
            similarity_index,
            target,
            fetcher,
            in_row,
            current: Doc::default(),
            exec_info: ExecInfo::default(),
        }
    }
}

/// A parent row's best listed chunk's score.
async fn score_row(
    fetcher: Option<&dyn DocFetcher>,
    target: &ChunkTarget,
    in_row: ListedInRow,
    row: &Doc,
    reads: &mut Reads,
) -> Result<Option<f64>> {
    let (Some(fetcher), Some(parent_id)) = (fetcher, row.doc_id()) else {
        return Ok(None);
    };
    let Some(listed) = listed_in_row(row, in_row, target) else {
        return Ok(None);
    };
    best_score_of(
        fetcher,
        target,
        parent_id,
        &listed,
        &RapidHashMap::new(),
        reads,
    )
    .await
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
        let mut reads = Reads::default();
        let score = score_row(
            self.fetcher.as_deref(),
            &self.target,
            self.in_row,
            &row,
            &mut reads,
        )
        .await?;
        self.exec_info.docs_fetched += reads.docs;
        self.exec_info.fields_fetched += reads.fields;
        self.exec_info.indexes_fetched += reads.index_lookups;
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
        // The chunk reads are this node's, on top of any its source reports here.
        let own = [
            ("docFetches", self.exec_info.docs_fetched),
            ("fieldFetches", self.exec_info.fields_fetched),
            ("indexFetches", self.exec_info.indexes_fetched),
        ];
        for (key, reads) in own {
            let child = obj.get(key).and_then(JsonValue::as_u64).unwrap_or(0);
            obj.insert(key.to_string(), serde_json::json!(child + reads));
        }
        JsonValue::Object(obj)
    }
}

#[cfg(test)]
#[path = "node_tests.rs"]
mod tests;
