//! The exact score: the best of the chunks a parent's current text lists.

use document::{Document, NormalValue};
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};
use schema::CHUNK_SOURCE_ID_FIELD;

use super::target::{ChunkTarget, Reads};
use crate::doc_stream::DocStream;
use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::plan::chunk_parent::{chunk_parent_id, verified_hash};
use crate::planner::index_selection::{IndexScanParams, IndexScanType};

/// The best score among the chunks `parent`'s current text lists, or `None`
/// when it lists none with a vector or isn't live.
pub(crate) async fn best_listed_score(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    parent: &Document,
    reads: &mut Reads,
) -> Result<Option<f64>> {
    let (Some(parent_id), Some(listed)) = (parent.id(), target.listed(parent)) else {
        return Ok(None);
    };
    let none = RapidHashMap::new();
    best_score_of(
        fetcher,
        target,
        &parent_id.to_string(),
        &listed,
        &none,
        reads,
    )
    .await
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

/// The best score among `parent_id`'s chunks whose hashes are in `listed`;
/// a hash whose stored copies `held` scored already isn't read again.
pub(crate) async fn best_score_of(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    parent_id: &str,
    listed: &RapidHashSet<String>,
    held: &RapidHashMap<String, Held>,
    reads: &mut Reads,
) -> Result<Option<f64>> {
    let mut best = held
        .iter()
        .filter(|(hash, _)| listed.contains(*hash))
        .map(|(_, held)| held.best)
        .reduce(f64::max);
    if listed.is_empty() {
        return Ok(best);
    }
    for chunk in listed_chunks(fetcher, target, parent_id, listed, held, reads).await? {
        reads.count(&chunk);
        // Superseded and forged chunks stay stored; only the current list counts.
        let is_listed = verified_hash(&chunk).is_some_and(|hash| listed.contains(hash));
        if !is_listed || chunk_parent_id(&chunk) != Some(parent_id) {
            continue;
        }
        if !target.readable(&chunk).await? {
            continue;
        }
        if let Some(score) = target.score(&chunk)? {
            best = Some(best.map_or(score, |best| best.max(score)));
        }
    }
    Ok(best)
}

/// The stored chunks of one parent with a hash in `listed`, less the hashes
/// `held` has every copy of.
async fn listed_chunks(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    parent_id: &str,
    listed: &RapidHashSet<String>,
    held: &RapidHashMap<String, Held>,
    reads: &mut Reads,
) -> Result<Vec<Document>> {
    let Some(index) = target
        .source_index
        .as_ref()
        .filter(|_| fetcher.supports_index_queries())
    else {
        return fetcher
            .get_by_field_value(&target.chunks, CHUNK_SOURCE_ID_FIELD, parent_id)
            .await;
    };
    let parent = NormalValue::String(parent_id.to_string());
    if let Some(entries) = fetcher
        .index_prefix_entries(&target.chunks, index, std::slice::from_ref(&parent))
        .await?
    {
        reads.index_lookups += 1;
        let mut by_hash: RapidHashMap<&str, Vec<u64>> = RapidHashMap::new();
        for entry in &entries {
            if let Some(hash) = entry_hash(&entry.values).filter(|hash| listed.contains(*hash)) {
                by_hash.entry(hash).or_default().push(entry.doc_short_id);
            }
        }
        let ids: Vec<u64> = by_hash
            .into_iter()
            .filter(|(hash, ids)| !all_held(held, hash, ids.len()))
            .flat_map(|(_, ids)| ids)
            .collect();
        return read_short_ids(fetcher, &target.chunks, &ids).await;
    }
    let mut ids = Vec::with_capacity(listed.len());
    for hash in listed {
        reads.index_lookups += 1;
        let found = fetcher
            .get_by_index_scan(
                &target.chunks,
                &IndexScanParams {
                    index_name: index.clone(),
                    scan_type: IndexScanType::ExactMatch {
                        values: vec![parent.clone(), NormalValue::String(hash.to_string())],
                    },
                    limit: None,
                    offset: 0,
                    value_filter: None,
                    cursor_seek: None,
                },
            )
            .await?;
        let found = found.into_doc_ids();
        if !all_held(held, hash, found.len()) {
            ids.extend(found);
        }
    }
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(fetcher.get_by_ids(&target.chunks, &ids).await?.into_docs())
}

/// Whether a search already scored every stored copy of `hash`.
fn all_held(held: &RapidHashMap<String, Held>, hash: &str, stored: usize) -> bool {
    held.get(hash).is_some_and(|held| held.copies >= stored)
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
