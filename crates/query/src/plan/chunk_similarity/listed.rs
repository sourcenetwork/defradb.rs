//! The exact score: the best of the chunks a parent's current text lists.

use document::{Document, NormalValue};
use rapidhash::RapidHashSet;
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
    best_score_of(fetcher, target, &parent_id.to_string(), &listed, reads).await
}

/// The best score among `parent_id`'s chunks whose hashes are in `listed`.
pub(crate) async fn best_score_of(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    parent_id: &str,
    listed: &RapidHashSet<String>,
    reads: &mut Reads,
) -> Result<Option<f64>> {
    let mut best: Option<f64> = None;
    for chunk in listed_chunks(fetcher, target, parent_id, listed).await? {
        reads.count(&chunk);
        // Superseded and forged chunks stay stored; only the current list counts.
        let is_listed = verified_hash(&chunk).is_some_and(|hash| listed.contains(hash));
        if !is_listed || chunk_parent_id(&chunk) != Some(parent_id) {
            continue;
        }
        if let Some(score) = target.score(&chunk)? {
            best = Some(best.map_or(score, |best| best.max(score)));
        }
    }
    Ok(best)
}

/// The stored chunks of one parent with a hash in `listed`, one index lookup per hash.
async fn listed_chunks(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    parent_id: &str,
    listed: &RapidHashSet<String>,
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
    let mut ids = Vec::with_capacity(listed.len());
    for hash in listed {
        let found = fetcher
            .get_by_index_scan(
                &target.chunks,
                &IndexScanParams {
                    index_name: index.clone(),
                    scan_type: IndexScanType::ExactMatch {
                        values: vec![
                            NormalValue::String(parent_id.to_string()),
                            NormalValue::String(hash.clone()),
                        ],
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
