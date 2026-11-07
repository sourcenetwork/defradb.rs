//! The routed search: the nearest chunks first, widened until they rank `k` parents.

use document::Document;
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};
use tracing::debug;

use super::best::best_per_parent;
use super::target::ChunkTarget;
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
    /// At least `k` parents, each with its best accepted chunk's score.
    Ranked(Vec<(Document, f64)>),
    /// The index ran out, or the candidate budget did, before `k` parents.
    Exhausted,
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
    /// Runs the search, returning it with how many index searches it took.
    ///
    /// A chunk ranks its parent only when its hash matches its text, the parent
    /// is live and accepted, and the parent's current list names it. When the
    /// nearest chunks read name `k` such parents, their best chunks rank them
    /// exactly as reading every chunk would.
    pub(crate) async fn run<C>(&self, check: &C) -> Result<(Searched, u64)>
    where
        C: ParentCheck,
    {
        let mut width = self.k.max(1);
        let mut searches = 0;
        let mut seen: RapidHashSet<u64> = RapidHashSet::new();
        let mut decided: RapidHashMap<String, Listed> = RapidHashMap::new();
        let mut parents: RapidHashMap<String, Document> = RapidHashMap::new();
        let mut hits: Vec<(String, f64)> = Vec::new();
        loop {
            if width > MAX_VECTOR_CANDIDATES {
                return Ok((Searched::Exhausted, searches));
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
            let exhausted = ids.len() < width;
            let fresh: Vec<u64> = ids.into_iter().filter(|id| seen.insert(*id)).collect();
            if fresh.is_empty() {
                return Ok((Searched::Exhausted, searches));
            }
            let chunks = self.read_chunks(&fresh).await?;
            self.decide_parents(&chunks, check, &mut decided, &mut parents)
                .await?;
            for chunk in &chunks {
                let (Some(parent), Some(hash)) = (chunk_parent_id(chunk), verified_hash(chunk))
                else {
                    continue;
                };
                let listed = decided
                    .get(parent)
                    .and_then(Option::as_ref)
                    .is_some_and(|listed| listed.contains(hash));
                if !listed {
                    continue;
                }
                if let Some(score) = self.target.score(chunk)? {
                    hits.push((parent.to_string(), score));
                }
            }
            let best = best_per_parent(hits.iter().cloned());
            debug!(
                chunks = %self.target.chunks,
                width,
                fresh = fresh.len(),
                parents = best.len(),
                "chunk vector index candidates"
            );
            if best.len() >= self.k {
                let ranked = best
                    .into_iter()
                    .filter_map(|(parent, score)| Some((parents.remove(&parent)?, score)))
                    .collect();
                return Ok((Searched::Ranked(ranked), searches));
            }
            if exhausted {
                return Ok((Searched::Exhausted, searches));
            }
            width = width.saturating_mul(2);
        }
    }

    async fn read_chunks(&self, short_ids: &[u64]) -> Result<Vec<Document>> {
        let mut stream = self
            .fetcher
            .stream_by_doc_short_ids(&self.target.chunks, short_ids, false)
            .await?;
        let mut chunks = Vec::with_capacity(short_ids.len());
        while let Some((chunk, _)) = stream.next().await? {
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
