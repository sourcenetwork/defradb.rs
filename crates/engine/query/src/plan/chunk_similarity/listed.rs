//! The exact score: the best of the chunks a parent's current text lists,
//! for a page of parents at a time.

use document::{Document, NormalValue};
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use super::target::{ChunkTarget, Reads};
use crate::doc_stream::DocStream;
use crate::error::{QueryError, Result};
use crate::fetcher::{DocFetcher, IndexEntryValues};
use crate::plan::chunk_parent::{chunk_parent_id, verified_hash};

/// The most parents one page scores.
const PAGE_PARENTS: usize = 128;
/// The listed chunks past which a page takes no more parents.
const PAGE_CHUNKS: usize = 4096;

/// Whether a page of `parents` parents that list `chunks` chunks is full.
pub(crate) fn page_full(parents: usize, chunks: usize) -> bool {
    parents >= PAGE_PARENTS || chunks >= PAGE_CHUNKS
}

/// The copies of one listed hash a search already scored.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Held {
    pub best: f64,
    pub copies: usize,
}

impl Held {
    pub(crate) fn new(score: f64) -> Self {
        Self {
            best: score,
            copies: 1,
        }
    }

    pub(crate) fn add(&mut self, score: f64) {
        self.best = self.best.max(score);
        self.copies += 1;
    }
}

/// One parent of a page: the hashes its text lists and the copies already scored.
pub(crate) struct Wanted<'a> {
    pub parent_id: &'a str,
    pub listed: &'a RapidHashSet<String>,
    pub held: &'a RapidHashMap<String, Held>,
}

impl Wanted<'_> {
    fn held_best(&self) -> Option<f64> {
        self.held
            .iter()
            .filter(|(hash, _)| self.listed.contains(*hash))
            .map(|(_, held)| held.best)
            .reduce(f64::max)
    }
}

/// Each wanted parent's best score among its chunks with a listed hash, or
/// `None` when none has a vector. The page costs one index read and one chunk read.
pub(crate) async fn best_scores(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    wanted: &[Wanted<'_>],
    reads: &mut Reads,
) -> Result<Vec<Option<f64>>> {
    let mut best: Vec<Option<f64>> = wanted.iter().map(Wanted::held_best).collect();
    let mut slots: RapidHashMap<&str, Vec<usize>> = RapidHashMap::new();
    for (slot, parent) in wanted.iter().enumerate() {
        if !parent.listed.is_empty() {
            slots.entry(parent.parent_id).or_default().push(slot);
        }
    }
    if slots.is_empty() {
        return Ok(best);
    }
    for chunk in page_chunks(fetcher, target, wanted, reads).await? {
        // Superseded and forged chunks stay stored; only the current list counts.
        let (Some(parent), Some(hash)) = (chunk_parent_id(&chunk), verified_hash(&chunk)) else {
            continue;
        };
        let Some(slots) = slots.get(parent) else {
            continue;
        };
        if !slots.iter().any(|&slot| wanted[slot].listed.contains(hash)) {
            continue;
        }
        if !target.readable(&chunk).await? {
            continue;
        }
        let Some(score) = target.score(&chunk)? else {
            continue;
        };
        for &slot in slots {
            if wanted[slot].listed.contains(hash) {
                best[slot] = Some(best[slot].map_or(score, |best| best.max(score)));
            }
        }
    }
    Ok(best)
}

/// The stored chunks of the page's listed hashes, less the hashes a parent
/// holds every copy of.
async fn page_chunks(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    wanted: &[Wanted<'_>],
    reads: &mut Reads,
) -> Result<Vec<Document>> {
    let index = target
        .source_index
        .as_ref()
        .filter(|_| fetcher.supports_index_queries());
    if let Some(index) = index {
        let mut parents: Vec<&str> = wanted.iter().map(|parent| parent.parent_id).collect();
        parents.sort_unstable();
        parents.dedup();
        // One prefix per parent: a seek per listed hash costs more than the
        // superseded entries it skips.
        let prefixes: Vec<Vec<NormalValue>> = parents
            .iter()
            .map(|parent| vec![NormalValue::String(parent.to_string())])
            .collect();
        if let Some(found) = fetcher
            .index_prefix_entries(&target.chunks, index, &prefixes)
            .await?
        {
            reads.index_lookups += 1;
            if found.len() != prefixes.len() {
                return Err(QueryError::internal(format!(
                    "{} answered {} of {} index prefixes",
                    target.chunks,
                    found.len(),
                    prefixes.len()
                )));
            }
            let entries: RapidHashMap<&str, &[IndexEntryValues]> = parents
                .iter()
                .copied()
                .zip(found.iter().map(Vec::as_slice))
                .collect();
            let mut seen = RapidHashSet::new();
            let mut ids = Vec::new();
            for parent in wanted {
                let Some(stored) = entries.get(parent.parent_id) else {
                    continue;
                };
                let mut by_hash: RapidHashMap<&str, Vec<u64>> = RapidHashMap::new();
                for entry in stored.iter() {
                    if let Some(hash) =
                        entry_hash(&entry.values).filter(|hash| parent.listed.contains(*hash))
                    {
                        by_hash.entry(hash).or_default().push(entry.doc_short_id);
                    }
                }
                for (hash, stored) in by_hash {
                    if !all_held(parent.held, hash, stored.len()) {
                        ids.extend(stored.into_iter().filter(|id| seen.insert(*id)));
                    }
                }
            }
            let chunks = read_short_ids(fetcher, &target.chunks, &ids).await?;
            chunks.iter().for_each(|chunk| reads.count(chunk));
            return Ok(chunks);
        }
    }
    // Without index entries one read of the chunk collection serves the whole page.
    let chunks = fetcher.get_all(&target.chunks).await?;
    chunks.iter().for_each(|chunk| reads.count(chunk));
    Ok(chunks)
}

/// The hash an entry of the `[_sourceID, hash]` index holds.
fn entry_hash(values: &[NormalValue]) -> Option<&str> {
    // An index key does not record that the hash was a string.
    match values.get(1)? {
        NormalValue::String(hash) => Some(hash),
        NormalValue::Bytes(hash) => std::str::from_utf8(hash).ok(),
        _ => None,
    }
}

/// Whether a search already scored every stored copy of `hash`.
fn all_held(held: &RapidHashMap<String, Held>, hash: &str, stored: usize) -> bool {
    held.get(hash).is_some_and(|held| held.copies >= stored)
}

async fn read_short_ids(
    fetcher: &dyn DocFetcher,
    collection: &str,
    ids: &[u64],
) -> Result<Vec<Document>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let stream = fetcher
        .stream_by_doc_short_ids(collection, ids, false)
        .await?;
    drain(stream, ids.len()).await
}

/// Every document `stream` yields, closing it even when a read fails.
pub(super) async fn drain(
    mut stream: Box<dyn DocStream>,
    capacity: usize,
) -> Result<Vec<Document>> {
    let mut docs = Vec::with_capacity(capacity);
    let read = loop {
        match stream.next().await {
            Ok(Some((doc, _))) => docs.push(doc),
            Ok(None) => break Ok(()),
            Err(error) => break Err(error),
        }
    };
    let closed = stream.close().await;
    read?;
    closed?;
    Ok(docs)
}

#[cfg(test)]
#[path = "listed_tests.rs"]
mod tests;
