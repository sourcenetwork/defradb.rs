//! The routed search: the nearest chunks first, widened until they rank `k` parents.

use document::Document;
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};
use tracing::debug;

use super::best::best_per_parent;
use super::listed::best_score_of;
use super::target::{ChunkTarget, Reads};
use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::plan::chunk_parent::{chunk_parent_id, parent_hashes, verified_hash};
use crate::plan::scan::MAX_VECTOR_CANDIDATES;

/// Whether the query may return a parent: its filter and requested document IDs.
pub(crate) trait ParentCheck {
    fn accepts(&self, parent: &Document) -> Result<bool>;
}

/// What a routed search found.
pub(crate) enum Searched {
    /// At least `k` parents, each with its exact best listed chunk's score.
    Ranked(Vec<(Document, f64)>),
    /// The search gave up before naming `k` parents.
    Exhausted(GaveUp),
}

/// Why a routed search gave up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GaveUp {
    /// Widening would pass the candidate budget.
    Budget,
    /// The index returned nothing it had not returned already.
    NothingNew,
    /// The index returned fewer chunks than asked for.
    RanOut,
}

/// A finished search, with what it cost.
pub(crate) struct Search {
    pub searched: Searched,
    pub searches: u64,
    pub reads: Reads,
}

/// One routed search over a chunk collection's vector index.
pub(crate) struct ChunkSearch<'a> {
    pub fetcher: &'a dyn DocFetcher,
    pub target: &'a ChunkTarget,
    pub index_id: u32,
    /// How many distinct parents the page needs.
    pub k: usize,
}

/// The chunk hashes an accepted parent's current text lists; `None` for a
/// parent that is gone, deleted or filtered out.
type Listed = Option<RapidHashSet<String>>;

impl ChunkSearch<'_> {
    /// Finds `k` parents through the index; an approximate index may miss one, never mis-score it.
    pub(crate) async fn run<C>(&self, check: &C) -> Result<Search>
    where
        C: ParentCheck,
    {
        let mut width = self.k.max(1);
        let mut searches = 0;
        let mut reads = Reads::default();
        let mut skipped = 0usize;
        let mut seen: RapidHashSet<u64> = RapidHashSet::new();
        let mut decided: RapidHashMap<String, Listed> = RapidHashMap::new();
        let mut parents: RapidHashMap<String, Document> = RapidHashMap::new();
        let mut hits: Vec<(String, f64)> = Vec::new();
        let gave_up = loop {
            if width > MAX_VECTOR_CANDIDATES {
                break GaveUp::Budget;
            }
            let ids = self
                .fetcher
                .vector_search(
                    &self.target.chunks,
                    self.index_id,
                    &self.target.vector,
                    width,
                    None,
                )
                .await?;
            searches += 1;
            let ran_out = ids.len() < width;
            let fresh: Vec<u64> = ids.into_iter().filter(|id| seen.insert(*id)).collect();
            if fresh.is_empty() {
                break GaveUp::NothingNew;
            }
            let chunks = self.read_chunks(&fresh, &mut reads).await?;
            self.decide_parents(&chunks, check, &mut decided, &mut parents, &mut reads)
                .await?;
            for chunk in &chunks {
                let listed = match (chunk_parent_id(chunk), verified_hash(chunk)) {
                    (Some(parent), Some(hash)) => decided
                        .get(parent)
                        .and_then(Option::as_ref)
                        .is_some_and(|listed| listed.contains(hash)),
                    _ => false,
                };
                let score = match listed {
                    true => self.target.score(chunk)?,
                    false => None,
                };
                match (score, chunk_parent_id(chunk)) {
                    (Some(score), Some(parent)) => hits.push((parent.to_string(), score)),
                    _ => skipped += 1,
                }
            }
            let best = best_per_parent(hits.iter().cloned());
            debug!(
                chunks = %self.target.chunks,
                width,
                fresh = fresh.len(),
                skipped,
                parents = best.len(),
                "chunk vector index candidates"
            );
            if best.len() >= self.k {
                let mut ranked = Vec::with_capacity(best.len());
                for (id, found) in best {
                    let (Some(parent), Some(Some(listed))) =
                        (parents.remove(&id), decided.get(&id))
                    else {
                        continue;
                    };
                    // The index may have missed a better chunk of this parent.
                    let exact = best_score_of(self.fetcher, self.target, &id, listed, &mut reads)
                        .await?
                        .unwrap_or(found);
                    ranked.push((parent, exact));
                }
                return Ok(Search {
                    searched: Searched::Ranked(ranked),
                    searches,
                    reads,
                });
            }
            if ran_out {
                break GaveUp::RanOut;
            }
            width = width.saturating_mul(2);
        };
        debug!(chunks = %self.target.chunks, ?gave_up, searches, skipped, "chunk vector search gave up");
        Ok(Search {
            searched: Searched::Exhausted(gave_up),
            searches,
            reads,
        })
    }

    async fn read_chunks(&self, short_ids: &[u64], reads: &mut Reads) -> Result<Vec<Document>> {
        let mut stream = self
            .fetcher
            .stream_by_doc_short_ids(&self.target.chunks, short_ids, false)
            .await?;
        let mut chunks = Vec::with_capacity(short_ids.len());
        while let Some((chunk, _)) = stream.next().await? {
            reads.count(&chunk);
            chunks.push(chunk);
        }
        stream.close().await?;
        Ok(chunks)
    }

    /// Reads, in one batch, each parent these chunks name for the first time.
    async fn decide_parents<C>(
        &self,
        chunks: &[Document],
        check: &C,
        decided: &mut RapidHashMap<String, Listed>,
        parents: &mut RapidHashMap<String, Document>,
        reads: &mut Reads,
    ) -> Result<()>
    where
        C: ParentCheck,
    {
        let mut new: Vec<String> = chunks
            .iter()
            .filter_map(chunk_parent_id)
            .filter(|parent| !decided.contains_key(*parent))
            .map(str::to_string)
            .collect();
        new.sort_unstable();
        new.dedup();
        if new.is_empty() {
            return Ok(());
        }
        for parent in self
            .fetcher
            .get_by_ids(&self.target.parent, &new)
            .await?
            .into_docs()
        {
            reads.count(&parent);
            let Some(id) = parent.id().map(ToString::to_string) else {
                continue;
            };
            let listed = match parent_hashes(&parent, &self.target.chunked) {
                Some(listed) if check.accepts(&parent)? => Some(listed),
                _ => None,
            };
            if listed.is_some() {
                parents.insert(id.clone(), parent);
            }
            decided.insert(id, listed);
        }
        for parent in new {
            decided.entry(parent).or_insert(None);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
