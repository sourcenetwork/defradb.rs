//! The parent scan of a routed chunked `SIMILARITY`: parents come from the chunk index.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use document::Document;
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};
use schema::CollectionVersion;
use serde_json::Value as JsonValue;

use super::listed::{best_scores, page_full, Held, Wanted};
use super::search::{ChunkSearch, GaveUp, Known, ParentCheck, Searched};
use super::target::{ChunkTarget, Reads};
use crate::doc_stream::DocStream;
use crate::document::{document_to_plan_doc, DocumentMapping};
use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::planner::{Doc, ExecInfo, PlanNode};

/// The parent scan of a routed chunked similarity, scoring each parent by its best chunk.
pub struct ChunkSimilarityScanNode {
    collection: CollectionVersion,
    mapping: DocumentMapping,
    target: ChunkTarget,
    index_id: u32,
    k: usize,
    similarity_index: usize,
    fetcher: Arc<dyn DocFetcher>,
    ranked: VecDeque<Doc>,
    fallback: Option<Box<dyn DocStream>>,
    /// What the search learned about the parents it read, so the fallback reads them no further.
    known: RapidHashMap<String, Known>,
    searched: bool,
    gave_up: Option<GaveUp>,
    current: Doc,
    exec_info: ExecInfo,
}

/// Drops deleted parents. The planner routes only a query with no filter and no docID.
struct Accepts;

impl ParentCheck for Accepts {
    fn accepts(&self, parent: &Document) -> Result<bool> {
        Ok(!parent.is_deleted())
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
        target.warn_without_splitter();
        Self {
            collection,
            mapping,
            target,
            index_id,
            k,
            similarity_index,
            fetcher,
            ranked: VecDeque::new(),
            fallback: None,
            known: RapidHashMap::new(),
            searched: false,
            gave_up: None,
            current: Doc::default(),
            exec_info: ExecInfo::default(),
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
        self.exec_info.indexes_fetched += reads.index_lookups;
    }

    /// Scores the fallback's next page of accepted parents together, in scan order.
    async fn score_fallback_page(&mut self) -> Result<()> {
        let Some(stream) = self.fallback.as_mut() else {
            return Ok(());
        };
        let mut reads = Reads::default();
        let mut page: Vec<(Document, Option<Cow<'_, RapidHashSet<String>>>)> = Vec::new();
        let mut chunks = 0;
        let mut drained = false;
        while !page_full(page.len(), chunks) {
            let Some((parent, _)) = stream.next().await? else {
                drained = true;
                break;
            };
            reads.count(&parent);
            if !Accepts.accepts(&parent)? {
                continue;
            }
            let listed = reuse_listed(&self.known, &self.target, &parent)
                .map(Cow::Borrowed)
                .or_else(|| self.target.listed(&parent).map(Cow::Owned));
            chunks += listed.as_ref().map_or(0, |listed| listed.len());
            page.push((parent, listed));
        }
        let none = RapidHashMap::new();
        let ids: Vec<Option<String>> = page
            .iter()
            .map(|(parent, _)| parent.id().map(ToString::to_string))
            .collect();
        let (slots, wanted): (Vec<usize>, Vec<Wanted<'_>>) = page
            .iter()
            .zip(&ids)
            .enumerate()
            .filter_map(|(slot, ((_, listed), id))| {
                let id = id.as_deref()?;
                let wanted = Wanted {
                    parent_id: id,
                    listed: listed.as_deref()?,
                    held: held_of(&self.known, id).unwrap_or(&none),
                };
                Some((slot, wanted))
            })
            .unzip();
        let mut scores = vec![None; page.len()];
        if !wanted.is_empty() {
            let found =
                best_scores(self.fetcher.as_ref(), &self.target, &wanted, &mut reads).await?;
            for (slot, score) in slots.into_iter().zip(found) {
                scores[slot] = score;
            }
        }
        drop(wanted);
        let mut rows = VecDeque::with_capacity(page.len());
        for ((parent, _), score) in page.iter().zip(scores) {
            rows.push_back(self.scored(parent, score)?);
        }
        drop(page);
        self.count(reads);
        self.ranked = rows;
        if drained {
            if let Some(mut stream) = self.fallback.take() {
                stream.close().await?;
            }
        }
        Ok(())
    }
}

/// The hashes the search cut from `parent`'s text, while that text is unchanged.
fn reuse_listed<'a>(
    known: &'a RapidHashMap<String, Known>,
    target: &ChunkTarget,
    parent: &Document,
) -> Option<&'a RapidHashSet<String>> {
    let known = known.get(&parent.id()?.to_string())?;
    let field = &target.chunked.field_name;
    (known.parent.get(field) == parent.get(field)).then_some(&known.listed)
}

fn held_of<'a>(
    known: &'a RapidHashMap<String, Known>,
    id: &str,
) -> Option<&'a RapidHashMap<String, Held>> {
    known.get(id).map(|known| &known.held)
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
        .run(&Accepts)
        .await?;
        self.searched = search.searches > 0;
        self.exec_info.indexes_fetched += search.searches;
        self.count(search.reads);
        match search.searched {
            Searched::Ranked(parents) => {
                let mut rows = VecDeque::with_capacity(parents.len());
                for (parent, score) in &parents {
                    rows.push_back(self.scored(parent, Some(*score))?);
                }
                self.ranked = rows;
                self.fallback = None;
                self.known = RapidHashMap::new();
                self.gave_up = None;
            }
            Searched::Exhausted { gave_up, known } => {
                self.gave_up = Some(gave_up);
                self.ranked = VecDeque::new();
                self.known = known;
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
        // A page can hold no accepted parent, so read on until one does or the scan ends.
        while self.ranked.is_empty() && self.fallback.is_some() {
            self.score_fallback_page().await?;
        }
        let Some(row) = self.ranked.pop_front() else {
            return Ok(false);
        };
        self.current = row;
        Ok(true)
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
        serde_json::json!({
            "filter": JsonValue::Null,
            "collectionName": self.collection.name,
            "collectionID": self.collection.version_id,
            "prefixes": [format!("/{}", self.collection.resolved_root_id())],
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
        // A search that gave up scored every parent, so the index did not answer.
        if let Some(gave_up) = self.gave_up {
            obj["fallback"] = JsonValue::String(gave_up.name().to_string());
        }
        obj
    }
}
