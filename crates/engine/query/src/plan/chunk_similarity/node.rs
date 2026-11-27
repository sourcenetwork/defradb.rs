//! The unrouted chunked `SIMILARITY`: each parent scored from its listed chunks.

use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};

use super::listed::{best_scores, page_full, Wanted};
use super::target::{ChunkTarget, Reads};
use crate::document::DocumentMapping;
use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::planner::{index_selection::CursorSeek, Doc, ExecInfo, PlanNode};

/// Scores each parent row by its best listed chunk, as `SimilarityNode` scores its own vector.
pub struct ChunkSimilarityNode {
    source: Box<dyn PlanNode>,
    mapping: DocumentMapping,
    similarity_index: usize,
    target: ChunkTarget,
    fetcher: Option<Arc<dyn DocFetcher>>,
    in_row: ListedInRow,
    scored: VecDeque<Doc>,
    drained: bool,
    current: Doc,
    exec_info: ExecInfo,
}

/// Where a parent row holds its chunked text.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ListedInRow {
    pub text: usize,
}

/// The chunk hashes a live parent row's text uses, or `None` when it has no text.
pub(crate) fn listed_in_row(
    row: &Doc,
    at: ListedInRow,
    target: &ChunkTarget,
) -> Option<RapidHashSet<String>> {
    // A deleted parent shows no chunks, even ones a merge has not removed yet.
    if row.is_deleted() {
        return None;
    }
    target.listed_in(row.get(at.text)?.as_str()?)
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
        target.warn_without_splitter();
        Self {
            source,
            mapping,
            similarity_index,
            target,
            fetcher,
            in_row,
            scored: VecDeque::new(),
            drained: false,
            current: Doc::default(),
            exec_info: ExecInfo::default(),
        }
    }

    /// Pulls a page of rows from the source and scores them together.
    async fn score_page(&mut self) -> Result<()> {
        let mut rows = Vec::new();
        let mut listed = Vec::new();
        let mut chunks = 0;
        while !page_full(rows.len(), chunks) {
            if !self.source.next().await? {
                self.drained = true;
                break;
            }
            let row = self.source.value().deep_clone();
            let hashes = listed_in_row(&row, self.in_row, &self.target);
            chunks += hashes.as_ref().map_or(0, RapidHashSet::len);
            rows.push(row);
            listed.push(hashes);
        }
        let mut reads = Reads::default();
        let scores = page_scores(
            self.fetcher.as_deref(),
            &self.target,
            &rows,
            &listed,
            &mut reads,
        )
        .await?;
        self.exec_info.docs_fetched += reads.docs;
        self.exec_info.fields_fetched += reads.fields;
        self.exec_info.indexes_fetched += reads.index_lookups;
        for (mut row, score) in rows.into_iter().zip(scores) {
            let score = score
                .and_then(serde_json::Number::from_f64)
                .map_or(JsonValue::Null, JsonValue::Number);
            row.set(self.similarity_index, score);
            self.scored.push_back(row);
        }
        Ok(())
    }
}

/// Each row's best listed chunk's score, read for the whole page at once.
async fn page_scores(
    fetcher: Option<&dyn DocFetcher>,
    target: &ChunkTarget,
    rows: &[Doc],
    listed: &[Option<RapidHashSet<String>>],
    reads: &mut Reads,
) -> Result<Vec<Option<f64>>> {
    let mut scores = vec![None; rows.len()];
    let Some(fetcher) = fetcher else {
        return Ok(scores);
    };
    let none = RapidHashMap::new();
    let (slots, wanted): (Vec<usize>, Vec<Wanted<'_>>) = rows
        .iter()
        .zip(listed)
        .enumerate()
        .filter_map(|(slot, (row, listed))| {
            let wanted = Wanted {
                parent_id: row.doc_id()?,
                listed: listed.as_ref()?,
                held: &none,
            };
            Some((slot, wanted))
        })
        .unzip();
    if wanted.is_empty() {
        return Ok(scores);
    }
    let found = best_scores(fetcher, target, &wanted, reads).await?;
    for (slot, score) in slots.into_iter().zip(found) {
        scores[slot] = score;
    }
    Ok(scores)
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PlanNode for ChunkSimilarityNode {
    async fn init(&mut self) -> Result<()> {
        self.exec_info = ExecInfo::default();
        self.scored.clear();
        self.drained = false;
        self.source.init().await
    }

    async fn start(&mut self) -> Result<()> {
        self.source.start().await
    }

    async fn next(&mut self) -> Result<bool> {
        self.exec_info.iterations += 1;
        if self.scored.is_empty() && !self.drained {
            self.score_page().await?;
        }
        let Some(row) = self.scored.pop_front() else {
            return Ok(false);
        };
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
