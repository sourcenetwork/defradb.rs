//! Entry-count estimates that let index selection prefer selective indexes.
//!
//! The shape score alone cannot tell a condition that matches every document
//! (a field all documents share) from one that matches a handful, so a filter
//! naming both may scan the whole collection. Before planning, each usable
//! index's scan is counted up to [`ESTIMATE_CAP`] entries.

use rapidhash::RapidHashMap;
use schema::CollectionVersion;

use crate::error::Result;
use crate::fetcher::DocFetcher;
use crate::mapper::{Filter, Select};
use crate::plan::ScanNode;

use super::conditions::select_best_index;
use super::filter_to_scan::filter_to_index_scan;

/// Maximum work budget per candidate; exhausted scans tie at this initial cap.
pub const ESTIMATE_CAP: u64 = 1024;

/// Capped entry counts by index name, for one filter on one collection.
pub type IndexEstimates = RapidHashMap<String, u64>;

/// Estimate each usable index's scan with at most [`ESTIMATE_CAP`] work units.
/// Later caps shrink to the best count plus one, preserving winners and ties.
///
/// Returns an empty map when fewer than two indexes are usable (there is no
/// choice to inform) or when the fetcher cannot estimate.
pub async fn estimate_filter_indexes(
    fetcher: &dyn DocFetcher,
    collection: &CollectionVersion,
    filter: &Filter,
) -> Result<IndexEstimates> {
    let mut estimates = IndexEstimates::default();
    if !fetcher.supports_index_queries() {
        return Ok(estimates);
    }
    let mut scans: Vec<_> = collection
        .indexes
        .iter()
        .filter_map(|index| filter_to_index_scan(filter, index, None, &collection.fields, None, 0))
        .collect();
    if scans.len() < 2 {
        return Ok(estimates);
    }
    // Try the shape winner first (especially unique equality lookups). Once
    // a small scan is known, competitors only need one more entry to lose.
    if let Some(best) = select_best_index(filter, &collection.indexes) {
        if let Some(position) = scans
            .iter()
            .position(|params| params.index_name == best.name)
        {
            scans.swap(0, position);
        }
    }
    let mut cap = ESTIMATE_CAP;
    for params in scans {
        match fetcher
            .estimate_index_scan(&collection.name, &params, cap)
            .await?
        {
            Some(count) => {
                cap = cap.min(count.saturating_add(1));
                estimates.insert(params.index_name, count);
            }
            None => return Ok(IndexEstimates::default()),
        }
    }
    Ok(estimates)
}

/// Estimates for one select's filter, so the planner applies them to that select only.
#[derive(Debug, Clone)]
pub struct SelectEstimates {
    pub collection_name: String,
    pub filter: Filter,
    pub estimates: IndexEstimates,
}

impl SelectEstimates {
    /// The estimates for `select`, if they were computed for its collection and filter.
    pub fn for_select(&self, select: &Select) -> Option<&IndexEstimates> {
        (select.cursor_params.is_none()
            && self.collection_name == select.collection_name
            && select.filter.as_ref() == Some(&self.filter))
        .then_some(&self.estimates)
    }
}

/// Estimate the usable indexes for `select`'s filter.
///
/// Cursor pages skip estimation: a cursor's seek key belongs to the index it
/// was issued on, so the choice must not change between pages.
pub async fn estimate_select(
    fetcher: &dyn DocFetcher,
    collection: &CollectionVersion,
    select: &Select,
) -> Result<Option<SelectEstimates>> {
    let Some(filter) = select.filter.as_ref() else {
        return Ok(None);
    };
    if select.cursor_params.is_some()
        || (!select.show_deleted
            && ScanNode::point_id_for(select.doc_ids.as_deref(), Some(filter)).is_some())
    {
        return Ok(None);
    }
    let estimates = estimate_filter_indexes(fetcher, collection, filter).await?;
    Ok((!estimates.is_empty()).then(|| SelectEstimates {
        collection_name: select.collection_name.clone(),
        filter: filter.clone(),
        estimates,
    }))
}
