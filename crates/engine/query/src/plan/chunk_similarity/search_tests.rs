//! The routed search against a fake index whose chunks come nearest first.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chunk_split::hash::text_hash;
use chunk_split::split::{split, ChunkConfig};
use defra_core::vector::Metric;
use document::{DocID, Document, NormalValue};
use schema::{CType, ChunkedField, CollectionBuilder, FieldDescription, FieldKind, ScalarKind};

use rapidhash::{HashMapExt, RapidHashMap};

use super::{ChunkSearch, GaveUp, ParentCheck, Searched};
use crate::access_hooks::{AppReadCheck, ReadRequest, ReadValidator};
use crate::doc_stream::{DocStream, VecStream};
use crate::error::{QueryError, Result};
use crate::fetcher::{DocFetcher, FetchByIdsResult, IndexEntryValues};
use crate::plan::chunk_similarity::listed::{best_scores, Wanted};
use crate::plan::chunk_similarity::target::{ChunkTarget, Reads};
use crate::plan::scan::MAX_VECTOR_CANDIDATES;

const PARENTS: &str = "Article";
const CHUNKS: &str = "Article__body_chunk";

/// One chunk in the index, nearest first by position.
#[derive(Clone)]
struct IndexedChunk {
    parent: &'static str,
    text: String,
    /// Whether the parent's current text holds it.
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
        // Long enough that the splitter never packs two into one chunk.
        text: format!("{parent} chunk at {nearness}, a paragraph of its own here."),
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

/// Another stored chunk with `of`'s text, so its hash, but a vector of its own.
fn copy_of(of: &IndexedChunk, nearness: f64) -> IndexedChunk {
    IndexedChunk {
        nearness,
        ..of.clone()
    }
}

/// How the fake finds a page's chunks.
#[derive(Clone, Copy, PartialEq)]
enum Lookup {
    /// No `[_sourceID, hash]` index: a read of every chunk.
    FieldValue,
    /// One read of the `[_sourceID, hash]` entries of every parent on the page.
    Entries,
    /// An index whose entries this fetcher can't read: a read of every chunk.
    NoEntries,
}

struct Index {
    chunks: Vec<IndexedChunk>,
    lookup: Lookup,
    deleted: Vec<&'static str>,
    searches: AtomicUsize,
    widest: AtomicUsize,
    /// How many times a whole chunk collection was read.
    full_reads: AtomicUsize,
    /// Whether a chunk read fails after its first chunk.
    fails: bool,
    closed: Arc<AtomicBool>,
}

impl Index {
    fn new(chunks: Vec<IndexedChunk>) -> Self {
        Self {
            chunks,
            lookup: Lookup::FieldValue,
            deleted: Vec::new(),
            searches: AtomicUsize::new(0),
            widest: AtomicUsize::new(0),
            full_reads: AtomicUsize::new(0),
            fails: false,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn chunk_doc(&self, short_id: u64) -> Document {
        let indexed = &self.chunks[short_id as usize];
        let mut doc = Document::with_id(
            DocID::new_v0_from_seed(&format!("chunk {short_id}")),
            loose(&["_sourceID", "text", "hash", "v"]),
        );
        let text = self.stored_text(indexed);
        let hash = match indexed.honest {
            true => text_hash(&text),
            false => text_hash("other text"),
        };
        let set = [
            ("_sourceID", NormalValue::String(parent_id(indexed.parent))),
            ("text", NormalValue::String(text)),
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
        let mut doc = Document::with_id(DocID::from_string(id).unwrap(), loose(&["body"]));
        doc.set("body", NormalValue::String(self.body_of(name)))
            .unwrap();
        doc.set_deleted(self.deleted.contains(&name));
        Some(doc)
    }
}

impl Index {
    /// A parent's text: one paragraph per chunk it holds, so its cut gives them back.
    fn body_of(&self, parent: &str) -> String {
        self.chunks
            .iter()
            .filter(|chunk| chunk.parent == parent && chunk.listed)
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// The text a chunk stores: the piece its parent's cut gives for it, or
    /// its own text when the parent's text no longer holds it.
    fn stored_text(&self, indexed: &IndexedChunk) -> String {
        if !indexed.listed {
            return indexed.text.clone();
        }
        let config = ChunkConfig::new(defra_core::DEFAULT_SPLITTER_VERSION, 64, 0).unwrap();
        split(&self.body_of(indexed.parent), &config)
            .into_iter()
            .map(|chunk| chunk.text)
            .find(|text| text.trim_end() == indexed.text)
            .expect("each paragraph is one chunk")
    }

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
    async fn get_all(&self, collection: &str) -> Result<Vec<Document>> {
        assert_eq!(collection, CHUNKS);
        self.full_reads.fetch_add(1, Ordering::Relaxed);
        Ok((0..self.chunks.len() as u64)
            .map(|id| self.chunk_doc(id))
            .collect())
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
        if self.fails {
            return Ok(Box::new(FailsAfterOne {
                first: Some(self.chunk_doc(short_ids[0])),
                closed: Arc::clone(&self.closed),
            }));
        }
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
        prefixes: &[Vec<NormalValue>],
    ) -> Result<Option<Vec<Vec<IndexEntryValues>>>> {
        assert_eq!(collection, CHUNKS);
        if self.lookup != Lookup::Entries {
            return Ok(None);
        }
        let entries = prefixes
            .iter()
            .map(|prefix| {
                let [parent] = prefix.as_slice() else {
                    unreachable!("a page asks for each parent's entries")
                };
                self.entries_of(parent.as_str().unwrap())
                    .map(|(doc_short_id, entry)| IndexEntryValues {
                        doc_short_id,
                        values: vec![parent.clone(), NormalValue::String(entry)],
                    })
                    .collect()
            })
            .collect();
        Ok(Some(entries))
    }

    async fn get_by_field_value(
        &self,
        _collection: &str,
        _field: &str,
        _value: &str,
    ) -> Result<Vec<Document>> {
        unreachable!("a page reads its chunks together")
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

/// A chunk stream that yields one chunk, then fails, and records its close.
struct FailsAfterOne {
    first: Option<Document>,
    closed: Arc<AtomicBool>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocStream for FailsAfterOne {
    async fn next(&mut self) -> Result<Option<(Document, bool)>> {
        match self.first.take() {
            Some(chunk) => Ok(Some((chunk, false))),
            None => Err(QueryError::execution("the chunk read failed")),
        }
    }

    async fn close(&mut self) -> Result<()> {
        self.closed.store(true, Ordering::Relaxed);
        Ok(())
    }
}

fn target(lookup: Lookup) -> ChunkTarget {
    ChunkTarget {
        parent: PARENTS.into(),
        chunked: ChunkedField {
            field_name: "body".into(),
            max_chars: 64,
            overlap: 0,
            splitter_version: defra_core::DEFAULT_SPLITTER_VERSION,
        },
        chunks: CHUNKS.into(),
        source_index: (lookup != Lookup::FieldValue).then(|| "source".to_string()),
        vector_indexes: Vec::new(),
        field: "v".into(),
        vector: vec![1.0, 0.0],
        metric: Metric::NegativeDot,
        app: None,
        splitter: ChunkConfig::new(defra_core::DEFAULT_SPLITTER_VERSION, 64, 0).ok(),
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

/// An app read validator on the chunk collection that refuses some chunks.
struct Refuses(Vec<String>);

#[async_trait]
impl ReadValidator for Refuses {
    fn governs(&self, _: &schema::CollectionVersion) -> bool {
        true
    }

    async fn may_read(&self, request: &ReadRequest<'_>) -> std::result::Result<bool, String> {
        Ok(!self.0.iter().any(|id| id == request.doc_id))
    }
}

/// The chunk collection's own app check, refusing the chunks at `short_ids`.
fn refusing(short_ids: &[u64]) -> Option<AppReadCheck> {
    let refused = short_ids
        .iter()
        .map(|id| DocID::new_v0_from_seed(&format!("chunk {id}")).to_string())
        .collect();
    let validator: Arc<dyn ReadValidator> = Arc::new(Refuses(refused));
    AppReadCheck::bind(Some(&validator), None, &loose(&[]))
}

/// Runs a search for `k` parents: the ranked parents by name, sorted, and what it read.
fn run<C>(index: &Index, k: usize, check: &C) -> (Option<Vec<(&'static str, f64)>>, Reads)
where
    C: ParentCheck,
{
    run_on(index, &target(index.lookup), k, check)
}

fn run_on<C>(
    index: &Index,
    target: &ChunkTarget,
    k: usize,
    check: &C,
) -> (Option<Vec<(&'static str, f64)>>, Reads)
where
    C: ParentCheck,
{
    let search = ChunkSearch {
        fetcher: index,
        target,
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

/// Why a search for `k` parents gave up, if it did.
fn gave_up(index: &Index, k: usize) -> Option<GaveUp> {
    let target = target(index.lookup);
    let search = ChunkSearch {
        fetcher: index,
        target: &target,
        index_id: 0,
        k,
    };
    match workers::block_on(search.run(&AnyParent))
        .unwrap()
        .unwrap()
        .searched
    {
        Searched::Exhausted(gave_up) => Some(gave_up),
        Searched::Ranked(_) => None,
    }
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
    assert_eq!(gave_up(&index, 3), Some(GaveUp::RanOut));
    assert_eq!(index.searches.load(Ordering::Relaxed), 1);
}

#[test]
fn the_search_stops_at_the_candidate_budget() {
    let chunks = (0..=MAX_VECTOR_CANDIDATES)
        .map(|_| chunk("A", 0.5))
        .collect();
    let index = Index::new(chunks);
    assert_eq!(gave_up(&index, 2), Some(GaveUp::Budget));
    assert_eq!(index.widest.load(Ordering::Relaxed), MAX_VECTOR_CANDIDATES);
}

/// B's and the weaker of A's chunks are found; A's best, missed by the index, is not.
fn index_missing_a_best(lookup: Lookup) -> Index {
    let mut index = Index::new(vec![chunk("B", 0.6), chunk("A", 0.5), missed("A", 0.95)]);
    index.lookup = lookup;
    index
}

#[test]
fn rescoring_reads_one_index_page_and_only_the_chunks_not_yet_seen() {
    let (ranked, reads) = run(&index_missing_a_best(Lookup::Entries), 2, &AnyParent);
    assert_eq!(ranked.unwrap(), vec![("A", 0.95), ("B", 0.6)]);
    // Two found chunks and their two parents, then only A's missed chunk.
    assert_eq!((reads.docs, reads.index_lookups), (5, 1));
}

#[test]
fn rescoring_without_index_entries_reads_the_chunks_once_for_the_page() {
    let index = index_missing_a_best(Lookup::NoEntries);
    let (ranked, reads) = run(&index, 2, &AnyParent);
    assert_eq!(ranked.unwrap(), vec![("A", 0.95), ("B", 0.6)]);
    assert_eq!((reads.docs, reads.index_lookups), (7, 0));
    assert_eq!(index.full_reads.load(Ordering::Relaxed), 1);
}

const LOOKUPS: [Lookup; 3] = [Lookup::FieldValue, Lookup::Entries, Lookup::NoEntries];

#[test]
fn two_copies_of_one_hash_read_together_rank_by_the_better() {
    let best = chunk("A", 0.9);
    for lookup in LOOKUPS {
        let mut index = Index::new(vec![best.clone(), copy_of(&best, 0.4), chunk("B", 0.3)]);
        index.lookup = lookup;
        assert_eq!(
            search(&index, 2, &AnyParent).unwrap(),
            vec![("A", 0.9), ("B", 0.3)]
        );
    }
}

#[test]
fn a_held_hash_is_read_again_when_the_index_missed_a_better_copy() {
    let found = chunk("A", 0.5);
    for lookup in LOOKUPS {
        let mut index = Index::new(vec![
            chunk("B", 0.6),
            found.clone(),
            IndexedChunk {
                indexed: false,
                ..copy_of(&found, 0.95)
            },
        ]);
        index.lookup = lookup;
        assert_eq!(
            search(&index, 2, &AnyParent).unwrap(),
            vec![("A", 0.95), ("B", 0.6)]
        );
    }
}

#[test]
fn a_chunk_its_own_app_check_refuses_never_scores_its_parent() {
    let index = Index::new(vec![chunk("A", 0.9), chunk("B", 0.6), chunk("A", 0.1)]);
    let target = target(index.lookup).with_app(refusing(&[0]));
    let ranked = run_on(&index, &target, 2, &AnyParent).0.unwrap();
    assert_eq!(ranked, vec![("A", 0.1), ("B", 0.6)]);
}

#[test]
fn the_exact_score_skips_a_chunk_its_own_app_check_refuses() {
    let index = Index::new(vec![chunk("A", 0.9), chunk("A", 0.1)]);
    let target = target(index.lookup).with_app(refusing(&[0]));
    let parent = index.parent_doc(&parent_id("A")).unwrap();
    let (id, listed) = (parent_id("A"), target.listed(&parent).unwrap());
    let held = RapidHashMap::new();
    let wanted = [Wanted {
        parent_id: &id,
        listed: &listed,
        held: &held,
    }];
    let scores = workers::block_on(best_scores(&index, &target, &wanted, &mut Reads::default()))
        .unwrap()
        .unwrap();
    assert_eq!(scores, vec![Some(0.1)]);
}

#[test]
fn a_chunk_read_that_fails_still_closes_its_stream() {
    let mut index = Index::new(vec![chunk("A", 0.9), chunk("B", 0.6)]);
    index.fails = true;
    let target = target(index.lookup);
    let search = ChunkSearch {
        fetcher: &index,
        target: &target,
        index_id: 0,
        k: 2,
    };
    let failed = workers::block_on(search.run(&AnyParent)).unwrap();
    assert!(failed.is_err());
    assert!(index.closed.load(Ordering::Relaxed));
}
