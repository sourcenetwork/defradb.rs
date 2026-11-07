//! The routed search against a fake index whose chunks come nearest first.

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use chunk_split::list::text_hash;
use defra_core::vector::Metric;
use document::{DocID, Document, NormalValue};
use schema::{CType, ChunkedField, CollectionBuilder, FieldDescription, FieldKind, ScalarKind};

use super::{ChunkSearch, ParentCheck, Searched};
use crate::doc_stream::{DocStream, VecStream};
use crate::error::Result;
use crate::fetcher::{DocFetcher, FetchByIdsResult};
use crate::plan::chunk_similarity::target::ChunkTarget;
use crate::plan::scan::MAX_VECTOR_CANDIDATES;

const PARENTS: &str = "Article";
const CHUNKS: &str = "Article__body_chunk";

/// One chunk in the index, nearest first by position.
#[derive(Clone)]
struct IndexedChunk {
    parent: &'static str,
    text: String,
    /// Whether the parent's current list names it.
    listed: bool,
    /// Whether its hash matches its text.
    honest: bool,
    /// Its vector: its similarity to the query vector `[1, 0]`.
    nearness: f64,
}

fn chunk(parent: &'static str, nearness: f64) -> IndexedChunk {
    IndexedChunk {
        parent,
        text: format!("{parent} chunk at {nearness}"),
        listed: true,
        honest: true,
        nearness,
    }
}

fn superseded(parent: &'static str, nearness: f64) -> IndexedChunk {
    IndexedChunk {
        listed: false,
        ..chunk(parent, nearness)
    }
}

fn forged(parent: &'static str, nearness: f64) -> IndexedChunk {
    IndexedChunk {
        honest: false,
        ..chunk(parent, nearness)
    }
}

struct Index {
    chunks: Vec<IndexedChunk>,
    searches: AtomicUsize,
    widest: AtomicUsize,
}

impl Index {
    fn new(chunks: Vec<IndexedChunk>) -> Self {
        Self {
            chunks,
            searches: AtomicUsize::new(0),
            widest: AtomicUsize::new(0),
        }
    }

    fn chunk_doc(&self, short_id: u64) -> Document {
        let indexed = &self.chunks[short_id as usize];
        let mut doc = Document::new(loose(&["_sourceID", "text", "hash", "v"]));
        let hash = match indexed.honest {
            true => text_hash(&indexed.text),
            false => text_hash("other text"),
        };
        let set = [
            ("_sourceID", NormalValue::String(parent_id(indexed.parent))),
            ("text", NormalValue::String(indexed.text.clone())),
            ("hash", NormalValue::String(hash)),
            (
                "v",
                NormalValue::Float64Array(vec![indexed.nearness, 1.0 - indexed.nearness]),
            ),
        ];
        for (field, value) in set {
            doc.set(field, value).unwrap();
        }
        doc
    }

    fn parent_doc(&self, id: &str) -> Option<Document> {
        let name = self
            .chunks
            .iter()
            .map(|chunk| chunk.parent)
            .find(|parent| parent_id(parent) == id)?;
        let body = format!("the body of {name}");
        let mut list = vec![text_hash(&body)];
        list.extend(
            self.chunks
                .iter()
                .filter(|chunk| chunk.parent == name && chunk.listed)
                .map(|chunk| text_hash(&chunk.text)),
        );
        let mut doc = Document::with_id(
            DocID::from_string(id).unwrap(),
            loose(&["body", "_body_chunks"]),
        );
        doc.set("body", NormalValue::String(body)).unwrap();
        doc.set("_body_chunks", NormalValue::StringArray(list))
            .unwrap();
        Some(doc)
    }
}

fn parent_id(name: &str) -> String {
    DocID::new_v0_from_seed(name).to_string()
}

fn loose(fields: &[&str]) -> schema::CollectionVersion {
    fields
        .iter()
        .enumerate()
        .fold(
            CollectionBuilder::new("Test", "bafytest"),
            |builder, (i, name)| {
                builder.field(FieldDescription::new(
                    i.to_string(),
                    *name,
                    FieldKind::Scalar(ScalarKind::None),
                    CType::LwwRegister,
                ))
            },
        )
        .build()
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocFetcher for Index {
    async fn get_all(&self, _collection: &str) -> Result<Vec<Document>> {
        unreachable!("the search never reads a whole collection")
    }

    async fn stream_all_with_deleted(
        &self,
        _collection: &str,
        _show_deleted: bool,
    ) -> Result<Box<dyn DocStream>> {
        unreachable!("the search never reads a whole collection")
    }

    async fn stream_by_doc_short_ids(
        &self,
        collection: &str,
        short_ids: &[u64],
        _show_deleted: bool,
    ) -> Result<Box<dyn DocStream>> {
        assert_eq!(collection, CHUNKS);
        let docs = short_ids
            .iter()
            .map(|id| (self.chunk_doc(*id), false))
            .collect();
        Ok(Box::new(VecStream::new(docs)))
    }

    async fn get_by_ids(&self, collection: &str, ids: &[String]) -> Result<FetchByIdsResult> {
        assert_eq!(collection, PARENTS);
        Ok(FetchByIdsResult::all_found(
            ids.iter().filter_map(|id| self.parent_doc(id)).collect(),
        ))
    }

    async fn get_by_field_value(
        &self,
        _collection: &str,
        _field: &str,
        _value: &str,
    ) -> Result<Vec<Document>> {
        unreachable!("the search reads chunks by short id")
    }

    fn supports_vector_search(&self) -> bool {
        true
    }

    async fn vector_search(
        &self,
        collection: &str,
        _index_id: u32,
        _query: &[f64],
        k: usize,
        _effort: Option<usize>,
    ) -> Result<Vec<u64>> {
        assert_eq!(collection, CHUNKS);
        self.searches.fetch_add(1, Ordering::Relaxed);
        self.widest.fetch_max(k, Ordering::Relaxed);
        Ok((0..self.chunks.len().min(k) as u64).collect())
    }
}

fn target() -> ChunkTarget {
    ChunkTarget {
        parent: PARENTS.into(),
        chunked: ChunkedField {
            field_name: "body".into(),
            max_chars: 64,
            overlap: 0,
        },
        chunks: CHUNKS.into(),
        source_index: None,
        field: "v".into(),
        vector: vec![1.0, 0.0],
        metric: Metric::NegativeDot,
    }
}

struct AnyParent;

impl ParentCheck for AnyParent {
    fn accepts(&self, _parent: &Document) -> Result<bool> {
        Ok(true)
    }
}

/// Rejects one parent, the way a filter does.
struct AllBut(&'static str);

impl ParentCheck for AllBut {
    fn accepts(&self, parent: &Document) -> Result<bool> {
        Ok(parent.id().map(ToString::to_string) != Some(parent_id(self.0)))
    }
}

/// Runs a search for `k` parents, returning the ranked parents by name.
fn search<C>(index: &Index, k: usize, check: &C) -> Option<Vec<(&'static str, f64)>>
where
    C: ParentCheck,
{
    let target = target();
    let search = ChunkSearch {
        fetcher: index,
        target: &target,
        index_id: 0,
        k,
    };
    let (searched, _) = workers::block_on(search.run(check)).unwrap().unwrap();
    let Searched::Ranked(parents) = searched else {
        return None;
    };
    let name = |id: String| {
        ["A", "B", "C"]
            .into_iter()
            .find(|name| parent_id(name) == id)
            .unwrap()
    };
    Some(
        parents
            .into_iter()
            .map(|(parent, score)| (name(parent.id().unwrap().to_string()), score))
            .collect(),
    )
}

#[test]
fn the_search_widens_until_its_chunks_name_k_parents() {
    let index = Index::new(vec![
        chunk("A", 0.9),
        chunk("A", 0.8),
        chunk("A", 0.7),
        chunk("B", 0.6),
        chunk("C", 0.5),
    ]);
    let ranked = search(&index, 2, &AnyParent).unwrap();
    assert_eq!(ranked, vec![("A", 0.9), ("B", 0.6)]);
    // Two chunks name only A, four still only A and B is the fourth: 2, then 4.
    assert_eq!(index.searches.load(Ordering::Relaxed), 2);
    assert_eq!(index.widest.load(Ordering::Relaxed), 4);
}

#[test]
fn a_superseded_or_forged_chunk_never_ranks_its_parent() {
    let index = Index::new(vec![
        superseded("A", 0.9),
        forged("A", 0.8),
        chunk("B", 0.6),
        chunk("A", 0.1),
    ]);
    // A ranks by its one listed chunk, far below the two chunks above it.
    assert_eq!(
        search(&index, 2, &AnyParent).unwrap(),
        vec![("B", 0.6), ("A", 0.1)]
    );
}

#[test]
fn a_rejected_parent_is_skipped_and_the_search_widens_past_it() {
    let index = Index::new(vec![chunk("A", 0.9), chunk("A", 0.8), chunk("B", 0.2)]);
    assert_eq!(search(&index, 1, &AllBut("A")).unwrap(), vec![("B", 0.2)]);
}

#[test]
fn an_index_that_runs_out_before_k_parents_reports_it() {
    let index = Index::new(vec![chunk("A", 0.9), chunk("B", 0.8)]);
    assert!(search(&index, 3, &AnyParent).is_none());
    assert_eq!(index.searches.load(Ordering::Relaxed), 1);
}

#[test]
fn the_search_stops_at_the_candidate_budget() {
    let chunks = (0..=MAX_VECTOR_CANDIDATES)
        .map(|_| chunk("A", 0.5))
        .collect();
    let index = Index::new(chunks);
    assert!(search(&index, 2, &AnyParent).is_none());
    assert_eq!(index.widest.load(Ordering::Relaxed), MAX_VECTOR_CANDIDATES);
}
