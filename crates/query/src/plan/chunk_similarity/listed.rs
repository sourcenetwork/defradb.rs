//! The exact score: the best of the chunks a parent's current text lists.

use document::{Document, NormalValue};
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};
use schema::CHUNK_SOURCE_ID_FIELD;

use super::target::{ChunkTarget, Reads};
use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::plan::chunk_parent::{chunk_parent_id, parent_hashes, verified_hash};
use crate::planner::index_selection::{IndexScanParams, IndexScanType};

/// The best score among the chunks `parent`'s current text lists, or `None`
/// when it lists none with a vector or isn't live.
pub(crate) async fn best_listed_score(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    parent: &Document,
    reads: &mut Reads,
) -> Result<Option<f64>> {
    let (Some(parent_id), Some(listed)) = (parent.id(), parent_hashes(parent, &target.chunked))
    else {
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

/// The best score among `parent_id`'s chunks whose hashes are in `listed`;
/// `held` gives the scores of listed chunks already read, which aren't read again.
pub(crate) async fn best_score_of(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    parent_id: &str,
    listed: &RapidHashSet<String>,
    held: &RapidHashMap<String, f64>,
    reads: &mut Reads,
) -> Result<Option<f64>> {
    let mut best = held
        .iter()
        .filter(|(hash, _)| listed.contains(*hash))
        .map(|(_, score)| *score)
        .reduce(f64::max);
    let wanted: RapidHashSet<&str> = listed
        .iter()
        .filter(|hash| !held.contains_key(*hash))
        .map(String::as_str)
        .collect();
    if wanted.is_empty() {
        return Ok(best);
    }
    for chunk in listed_chunks(fetcher, target, parent_id, &wanted, reads).await? {
        reads.count(&chunk);
        // Superseded and forged chunks stay stored; only the current list counts.
        let is_wanted = verified_hash(&chunk).is_some_and(|hash| wanted.contains(hash));
        if !is_wanted || chunk_parent_id(&chunk) != Some(parent_id) {
            continue;
        }
        if let Some(score) = target.score(&chunk)? {
            best = Some(best.map_or(score, |best| best.max(score)));
        }
    }
    Ok(best)
}

/// The stored chunks of one parent with a hash in `wanted`.
async fn listed_chunks(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    parent_id: &str,
    wanted: &RapidHashSet<&str>,
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
        let ids: Vec<u64> = entries
            .into_iter()
            .filter(|entry| entry_hash(&entry.values).is_some_and(|hash| wanted.contains(hash)))
            .map(|entry| entry.doc_short_id)
            .collect();
        return read_short_ids(fetcher, &target.chunks, &ids).await;
    }
    let mut ids = Vec::with_capacity(wanted.len());
    for hash in wanted {
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
        ids.extend(found.into_doc_ids());
    }
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(fetcher.get_by_ids(&target.chunks, &ids).await?.into_docs())
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
    let mut stream = fetcher
        .stream_by_doc_short_ids(collection, ids, false)
        .await?;
    let mut docs = Vec::with_capacity(ids.len());
    while let Some((doc, _)) = stream.next().await? {
        docs.push(doc);
    }
    stream.close().await?;
    Ok(docs)
}
