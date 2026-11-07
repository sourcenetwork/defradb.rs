//! The unrouted chunked `SIMILARITY`: each parent scored from its listed chunks.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use super::listed::best_listed_score;
use super::target::ChunkTarget;
use crate::document::DocumentMapping;
use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::plan::chunk_gate::get_projected;
use crate::plan::chunk_parent::parent_fields;
use crate::planner::{index_selection::CursorSeek, Doc, ExecInfo, PlanNode};

/// Scores each parent from its source by its best listed chunk, the way
/// `SimilarityNode` scores a document's own vector.
pub struct ChunkSimilarityNode {
    source: Box<dyn PlanNode>,
    mapping: DocumentMapping,
    similarity_index: usize,
    target: ChunkTarget,
    fetcher: Option<Arc<dyn DocFetcher>>,
    current: Doc,
    exec_info: ExecInfo,
}

/// A parent's best listed chunk's score, read through its current list.
async fn score_parent(
    fetcher: Option<&dyn DocFetcher>,
    target: &ChunkTarget,
    parent_id: &str,
) -> Result<Option<f64>> {
    let Some(fetcher) = fetcher else {
        return Ok(None);
    };
    let fields = parent_fields(&target.chunked);
    let Some(parent) = get_projected(fetcher, &target.parent, parent_id, &fields).await? else {
        return Ok(None);
    };
    best_listed_score(fetcher, target, &parent).await
}

impl ChunkSimilarityNode {
    pub(crate) fn new(
        source: Box<dyn PlanNode>,
        mapping: DocumentMapping,
        similarity_index: usize,
        target: ChunkTarget,
        fetcher: Option<Arc<dyn DocFetcher>>,
    ) -> Self {
        Self {
            source,
            mapping,
            similarity_index,
            target,
            fetcher,
            current: Doc::default(),
            exec_info: ExecInfo::default(),
        }
    }
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
        let score = match row.doc_id() {
            Some(id) => score_parent(self.fetcher.as_deref(), &self.target, id).await?,
            None => None,
        };
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
