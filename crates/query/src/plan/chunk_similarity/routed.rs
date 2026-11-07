//! The parent scan of a routed chunked `SIMILARITY`: parents come from the chunk index.

use std::sync::Arc;

use async_trait::async_trait;
use document::Document;
use schema::CollectionVersion;
use serde_json::Value as JsonValue;
use tracing::debug;

use super::listed::best_listed_score;
use super::search::{ChunkSearch, ParentCheck, Searched};
use super::target::{ChunkTarget, Reads};
use crate::doc_stream::DocStream;
use crate::document::{document_to_plan_doc, DocumentMapping};
use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::mapper::Filter;
use crate::planner::{Doc, ExecInfo, PlanNode};

/// The parent scan of a routed chunked similarity, scoring each parent by its best chunk.
pub struct ChunkSimilarityScanNode {
    collection: CollectionVersion,
    mapping: DocumentMapping,
    filter: Option<Filter>,
    doc_ids: Option<Vec<String>>,
    target: ChunkTarget,
    index_id: u32,
    k: usize,
    similarity_index: usize,
    fetcher: Arc<dyn DocFetcher>,
    ranked: std::vec::IntoIter<Doc>,
    fallback: Option<Box<dyn DocStream>>,
    searched: bool,
    current: Doc,
    exec_info: ExecInfo,
}

/// The parent filter and document IDs, checked on the parent's plan row.
struct Accepts<'a> {
    mapping: &'a DocumentMapping,
    filter: Option<&'a Filter>,
    doc_ids: Option<&'a [String]>,
}

impl ParentCheck for Accepts<'_> {
    fn accepts(&self, parent: &Document) -> Result<bool> {
        if parent.is_deleted() {
            return Ok(false);
        }
        let row = document_to_plan_doc(parent, self.mapping)?;
        if let Some(ids) = self.doc_ids {
            if !row
                .doc_id()
                .is_some_and(|id| ids.iter().any(|want| want == id))
            {
                return Ok(false);
            }
        }
        match self.filter {
            Some(filter) => filter.matches(row.fields(), self.mapping),
            None => Ok(true),
        }
    }
}

impl ChunkSimilarityScanNode {
    pub(crate) fn new(
        collection: CollectionVersion,
        mapping: DocumentMapping,
        target: ChunkTarget,
        index_id: u32,
        k: usize,
        similarity_index: usize,
        fetcher: Arc<dyn DocFetcher>,
    ) -> Self {
        Self {
            collection,
            mapping,
            filter: None,
            doc_ids: None,
            target,
            index_id,
            k,
            similarity_index,
            fetcher,
            ranked: Vec::new().into_iter(),
            fallback: None,
            searched: false,
            current: Doc::default(),
            exec_info: ExecInfo::default(),
        }
    }

    pub(crate) fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }

    pub(crate) fn with_doc_ids(mut self, doc_ids: Vec<String>) -> Self {
        if !doc_ids.is_empty() {
            self.doc_ids = Some(doc_ids);
        }
        self
    }

    fn accepts(&self) -> Accepts<'_> {
        Accepts {
            mapping: &self.mapping,
            filter: self.filter.as_ref(),
            doc_ids: self.doc_ids.as_deref(),
        }
    }

    fn scored(&self, parent: &Document, score: Option<f64>) -> Result<Doc> {
        let mut row = document_to_plan_doc(parent, &self.mapping)?;
        let score = score
            .and_then(serde_json::Number::from_f64)
            .map_or(JsonValue::Null, JsonValue::Number);
        row.set(self.similarity_index, score);
        Ok(row)
    }

    fn count(&mut self, reads: Reads) {
        self.exec_info.docs_fetched += reads.docs;
        self.exec_info.fields_fetched += reads.fields;
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PlanNode for ChunkSimilarityScanNode {
    async fn init(&mut self) -> Result<()> {
        self.exec_info = ExecInfo::default();
        let search = ChunkSearch {
            fetcher: self.fetcher.as_ref(),
            target: &self.target,
            index_id: self.index_id,
            k: self.k,
        }
        .run(&self.accepts())
        .await?;
        self.searched = search.searches > 0;
        self.exec_info.indexes_fetched += search.searches;
        self.count(search.reads);
        match search.searched {
            Searched::Ranked(parents) => {
                let mut rows = Vec::with_capacity(parents.len());
                for (parent, score) in &parents {
                    rows.push(self.scored(parent, Some(*score))?);
                }
                self.ranked = rows.into_iter();
                self.fallback = None;
            }
            Searched::Exhausted(gave_up) => {
                debug!(
                    collection = %self.collection.name,
                    chunks = %self.target.chunks,
                    ?gave_up,
                    "chunk vector index could not fill the page, scoring every parent"
                );
                self.ranked = Vec::new().into_iter();
                self.fallback = Some(
                    self.fetcher
                        .stream_all_with_deleted(&self.collection.name, false)
                        .await?,
                );
            }
        }
        Ok(())
    }

    async fn start(&mut self) -> Result<()> {
        Ok(())
    }

    async fn next(&mut self) -> Result<bool> {
        self.exec_info.iterations += 1;
        if let Some(row) = self.ranked.next() {
            self.current = row;
            return Ok(true);
        }
        loop {
            let pulled = match self.fallback.as_mut() {
                Some(stream) => stream.next().await?,
                None => None,
            };
            let Some((parent, _)) = pulled else {
                return Ok(false);
            };
            let mut reads = Reads::default();
            reads.count(&parent);
            if !self.accepts().accepts(&parent)? {
                self.count(reads);
                continue;
            }
            let score =
                best_listed_score(self.fetcher.as_ref(), &self.target, &parent, &mut reads).await?;
            self.count(reads);
            self.current = self.scored(&parent, score)?;
            return Ok(true);
        }
    }

    fn value(&self) -> &Doc {
        &self.current
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(mut stream) = self.fallback.take() {
            stream.close().await?;
        }
        Ok(())
    }

    fn source(&self) -> Option<&dyn PlanNode> {
        None
    }

    fn document_map(&self) -> &DocumentMapping {
        &self.mapping
    }

    fn kind(&self) -> &'static str {
        // The parent scan, narrowed by the chunk index, like a routed `scanNode`.
        "scanNode"
    }

    fn explain_inner(&self) -> JsonValue {
        let filter = match (&self.filter, &self.doc_ids) {
            (Some(filter), Some(_)) => crate::plan::strip_docid_from_filter(filter),
            (Some(filter), None) => filter.to_explain_json(),
            (None, _) => JsonValue::Null,
        };
        let prefixes = match self.doc_ids {
            Some(_) => Vec::new(),
            None => vec![format!("/{}", self.collection.resolved_root_id())],
        };
        serde_json::json!({
            "filter": filter,
            "collectionName": self.collection.name,
            "collectionID": self.collection.version_id,
            "prefixes": prefixes,
        })
    }

    fn exec_info(&self) -> ExecInfo {
        self.exec_info.clone()
    }

    fn explain_execute_inner(&self) -> JsonValue {
        let mut obj = serde_json::json!({
            "iterations": self.exec_info.iterations,
            "docFetches": self.exec_info.docs_fetched,
            "fieldFetches": self.exec_info.fields_fetched,
            "indexFetches": self.exec_info.indexes_fetched,
        });
        if let Some(name) = self
            .searched
            .then(|| self.target.vector_index_name(self.index_id))
            .flatten()
        {
            obj["vectorIndex"] = JsonValue::String(name.to_string());
        }
        obj
    }
}
