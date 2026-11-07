//! The exhaustive score: the best of the chunks a parent's current text lists.

use document::{Document, NormalValue};
use schema::CHUNK_SOURCE_ID_FIELD;

use super::target::ChunkTarget;
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
) -> Result<Option<f64>> {
    let (Some(parent_id), Some(listed)) = (parent.id(), parent_hashes(parent, &target.chunked))
    else {
        return Ok(None);
    };
    let parent_id = parent_id.to_string();
    let mut best: Option<f64> = None;
    for chunk in chunks_of(fetcher, target, &parent_id).await? {
        // Superseded and forged chunks stay stored; only the current list counts.
        let is_listed = verified_hash(&chunk).is_some_and(|hash| listed.contains(hash));
        if !is_listed || chunk_parent_id(&chunk) != Some(parent_id.as_str()) {
            continue;
        }
        if let Some(score) = target.score(&chunk)? {
            best = Some(best.map_or(score, |best| best.max(score)));
        }
    }
    Ok(best)
}

/// Every stored chunk of one parent, listed or not.
async fn chunks_of(
    fetcher: &dyn DocFetcher,
    target: &ChunkTarget,
    parent_id: &str,
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
    let found = fetcher
        .get_by_index_scan(
            &target.chunks,
            &IndexScanParams {
                index_name: index.clone(),
                scan_type: IndexScanType::PrefixScan {
                    prefix_values: vec![NormalValue::String(parent_id.to_string())],
                    reverse: false,
                },
                limit: None,
                offset: 0,
                value_filter: None,
                cursor_seek: None,
            },
        )
        .await?;
    Ok(fetcher
        .get_by_ids(&target.chunks, found.doc_ids())
        .await?
        .into_docs())
}
