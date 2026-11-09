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
use crate::fetcher::{DocFetcher, FetchByIdsResult, IndexEntryValues, IndexScanResult};
use crate::plan::chunk_similarity::target::{ChunkTarget, Reads};
use crate::plan::scan::MAX_VECTOR_CANDIDATES;
use crate::planner::index_selection::{IndexScanParams, IndexScanType};

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
    /// Whether the vector index returns it; an approximate index can miss one.
    indexed: bool,
    /// Its vector: its similarity to the query vector `[1, 0]`.
    nearness: f64,
}

fn chunk(parent: &'static str, nearness: f64) -> IndexedChunk {
    IndexedChunk {
        parent,
        text: format!("{parent} chunk at {nearness}"),
        listed: true,
        honest: true,
        indexed: true,
        nearness,
    }
}

fn superseded(parent: &'static str, nearness: f64) -> IndexedChunk {
    IndexedChunk {
        listed: false,
        ..chunk(parent, nearness)
    }
}

fn missed(parent: &'static str, nearness: f64) -> IndexedChunk {
    IndexedChunk {
        indexed: false,
        ..chunk(parent, nearness)
    }
}

fn forged(parent: &'static str, nearness: f64) -> IndexedChunk {
    IndexedChunk {
        honest: false,
        ..chunk(parent, nearness)
    }
}

/// How the fake looks up one parent's chunks.
#[derive(Clone, Copy, PartialEq)]
enum Lookup {
    /// By the `_sourceID` field, with no index.
    FieldValue,
    /// One `[_sourceID, hash]` prefix scan, reading the hashes from the entries.
    Entries,
    /// One `[_sourceID, hash]` exact match per hash.
    ExactMatch,
}

struct Index {
    chunks: Vec<IndexedChunk>,
    lookup: Lookup,
    deleted: Vec<&'static str>,
    searches: AtomicUsize,
    widest: AtomicUsize,
}

impl Index {
    fn new(chunks: Vec<IndexedChunk>) -> Self {
        Self {
            chunks,
            lookup: Lookup::FieldValue,
            deleted: Vec::new(),
            searches: AtomicUsize::new(0),
            widest: AtomicUsize::new(0),
        }
    }

    fn chunk_doc(&self, short_id: u64) -> Document {
        let indexed = &self.chunks[short_id as usize];
        let mut doc = Document::with_id(
            DocID::new_v0_from_seed(&format!("chunk {short_id}")),
            loose(&["_sourceID", "text", "hash", "v"]),
        );
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
        doc.set_deleted(self.deleted.contains(&name));
        Some(doc)
    }
}

impl Index {
    /// The `[_sourceID, hash]` entries of one parent's stored chunks.
    fn entries_of<'a>(&'a self, parent: &'a str) -> impl Iterator<Item = (u64, String)> + 'a {
        (0..self.chunks.len() as u64)
            .map(|id| self.chunk_doc(id))
            .enumerate()
            .filter(move |(_, chunk)| {
                chunk.get("_sourceID").and_then(NormalValue::as_str) == Some(parent)
            })
            .map(|(id, chunk)| {
                let hash = chunk.get("hash").and_then(NormalValue::as_str).unwrap();
                (id as u64, hash.to_string())
            })
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
        let docs = match collection {
            PARENTS => ids.iter().filter_map(|id| self.parent_doc(id)).collect(),
            _ => (0..self.chunks.len() as u64)
                .map(|id| self.chunk_doc(id))
                .filter(|chunk| ids.contains(&chunk.id().unwrap().to_string()))
                .collect(),
        };
        Ok(FetchByIdsResult::all_found(docs))
    }

    fn supports_index_queries(&self) -> bool {
        self.lookup != Lookup::FieldValue
    }

    async fn index_prefix_entries(
        &self,
        collection: &str,
        _index: &str,
        prefix: &[NormalValue],
    ) -> Result<Option<Vec<IndexEntryValues>>> {
        assert_eq!(collection, CHUNKS);
        if self.lookup != Lookup::Entries {
            return Ok(None);
        }
        Ok(Some(
            self.entries_of(prefix[0].as_str().unwrap())
                .map(|(doc_short_id, hash)| IndexEntryValues {
                    doc_short_id,
                    values: vec![prefix[0].clone(), NormalValue::String(hash)],
                })
                .collect(),
        ))
    }

    async fn get_by_index_scan(
        &self,
        collection: &str,
        params: &IndexScanParams,
    ) -> Result<IndexScanResult> {
        assert_eq!(collection, CHUNKS);
        let IndexScanType::ExactMatch { values } = &params.scan_type else {
            unreachable!("one listed chunk is an exact match")
        };
        let (parent, hash) = (values[0].as_str().unwrap(), values[1].as_str().unwrap());
        Ok(IndexScanResult::new(
            self.entries_of(parent)
                .filter(|(_, entry)| entry == hash)
                .map(|(id, _)| self.chunk_doc(id).id().unwrap().to_string())
                .collect(),
        ))
    }

    async fn get_by_field_value(
        &self,
        collection: &str,
        field: &str,
        value: &str,
    ) -> Result<Vec<Document>> {
        assert_eq!((collection, field), (CHUNKS, "_sourceID"));
        Ok((0..self.chunks.len() as u64)
            .filter(|id| parent_id(self.chunks[*id as usize].parent) == value)
            .map(|id| self.chunk_doc(id))
            .collect())
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
        Ok((0..self.chunks.len() as u64)
            .filter(|id| self.chunks[*id as usize].indexed)
            .take(k)
            .collect())
    }
}

fn target(lookup: Lookup) -> ChunkTarget {
    ChunkTarget {
        parent: PARENTS.into(),
        chunked: ChunkedField {
            field_name: "body".into(),
            max_chars: 64,
            overlap: 0,
        },
        chunks: CHUNKS.into(),
        source_index: (lookup != Lookup::FieldValue).then(|| "source".to_string()),
        vector_indexes: Vec::new(),
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

/// Runs a search for `k` parents: the ranked parents by name, sorted, and what it read.
fn run<C>(index: &Index, k: usize, check: &C) -> (Option<Vec<(&'static str, f64)>>, Reads)
where
    C: ParentCheck,
{
    let target = target(index.lookup);
    let search = ChunkSearch {
        fetcher: index,
        target: &target,
        index_id: 0,
        k,
    };
    let found = workers::block_on(search.run(check)).unwrap().unwrap();
    let Searched::Ranked(parents) = found.searched else {
        return (None, found.reads);
    };
    let name = |id: String| {
        ["A", "B", "C"]
            .into_iter()
            .find(|name| parent_id(name) == id)
            .unwrap()
    };
    let mut ranked: Vec<(&'static str, f64)> = parents
        .into_iter()
        .map(|(parent, score)| (name(parent.id().unwrap().to_string()), score))
        .collect();
    ranked.sort_by(|a, b| a.0.cmp(b.0));
    (Some(ranked), found.reads)
}

/// The parents a search for `k` ranks, by name, sorted.
fn search<C>(index: &Index, k: usize, check: &C) -> Option<Vec<(&'static str, f64)>>
where
    C: ParentCheck,
{
    run(index, k, check).0
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
    // The first two chunks are both A's; the first four reach B.
    assert_eq!(index.searches.load(Ordering::Relaxed), 2);
    assert_eq!(index.widest.load(Ordering::Relaxed), 4);
}

#[test]
fn a_superseded_chunk_never_ranks_its_parent() {
    let index = Index::new(vec![superseded("A", 0.9), chunk("B", 0.6), chunk("A", 0.1)]);
    assert_eq!(
        search(&index, 2, &AnyParent).unwrap(),
        vec![("A", 0.1), ("B", 0.6)]
    );
}

#[test]
fn a_forged_chunk_never_ranks_its_parent() {
    let index = Index::new(vec![forged("A", 0.9), chunk("B", 0.6), chunk("A", 0.1)]);
    assert_eq!(
        search(&index, 2, &AnyParent).unwrap(),
        vec![("A", 0.1), ("B", 0.6)]
    );
}

#[test]
fn a_parent_scores_its_best_chunk_even_when_the_index_misses_it() {
    let index = Index::new(vec![chunk("B", 0.6), chunk("A", 0.5), missed("A", 0.95)]);
    assert_eq!(
        search(&index, 2, &AnyParent).unwrap(),
        vec![("A", 0.95), ("B", 0.6)]
    );
}

#[test]
fn a_deleted_parent_never_ranks() {
    let mut index = Index::new(vec![chunk("A", 0.9), chunk("B", 0.6)]);
    index.deleted = vec!["A"];
    assert_eq!(search(&index, 1, &AnyParent).unwrap(), vec![("B", 0.6)]);
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

/// B's and the weaker of A's chunks are found; A's best, missed by the index, is not.
fn index_missing_a_best(lookup: Lookup) -> Index {
    let mut index = Index::new(vec![chunk("B", 0.6), chunk("A", 0.5), missed("A", 0.95)]);
    index.lookup = lookup;
    index
}

#[test]
fn rescoring_reads_one_prefix_scan_and_only_the_chunks_not_yet_seen() {
    let (ranked, reads) = run(&index_missing_a_best(Lookup::Entries), 2, &AnyParent);
    assert_eq!(ranked.unwrap(), vec![("A", 0.95), ("B", 0.6)]);
    // Two found chunks and their two parents, then only A's missed chunk.
    assert_eq!((reads.docs, reads.index_lookups), (5, 1));
}

#[test]
fn rescoring_without_index_entries_matches_each_unseen_hash_exactly() {
    let (ranked, reads) = run(&index_missing_a_best(Lookup::ExactMatch), 2, &AnyParent);
    assert_eq!(ranked.unwrap(), vec![("A", 0.95), ("B", 0.6)]);
    assert_eq!((reads.docs, reads.index_lookups), (5, 1));
}
