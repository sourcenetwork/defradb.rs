//! Capped index-entry counts for cost-aware index selection.
//!
//! Counting walks index keys without assembling documents. A shared budget
//! bounds returned keys and empty probes, including across IN and OR branches.

use query::planner::index_selection::IndexScanType;
use storage::corekv::{MaybeSend, Reader, Result};
use storage::index::IndexIterator;

use crate::index::manager::IndexType;

/// Count the entries `scan` would visit on `index`, stopping at `cap`.
///
/// Unfinished or unsupported scans report `cap`. Empty probes consume budget
/// too, so long IN lists cannot hide unbounded work behind zero matched entries.
pub(crate) async fn count_index_scan<R: Reader + MaybeSend>(
    index: &IndexType,
    reader: &R,
    scan: &IndexScanType,
    cap: usize,
) -> Result<usize> {
    let mut remaining = cap;
    let count = count_scan(index, reader, scan, &mut remaining).await?;
    // An exhausted work budget is not evidence that the scan is small.
    Ok(count.unwrap_or(cap))
}

/// Share a work budget across all probes, including empty IN/OR branches.
async fn count_scan<R: Reader + MaybeSend>(
    index: &IndexType,
    reader: &R,
    scan: &IndexScanType,
    remaining: &mut usize,
) -> Result<Option<usize>> {
    if *remaining == 0 {
        return Ok(None);
    }
    let count = match scan {
        IndexScanType::ExactMatch { values } => {
            index
                .get(reader, values)
                .await?
                .count_up_to(*remaining)
                .await?
        }
        IndexScanType::InScan {
            values,
            suffix_values,
        } => {
            let fields = index.description().fields.len();
            let full_key = !suffix_values.is_empty() && suffix_values.len() == fields - 1;
            let mut total = 0;
            for value in values {
                if *remaining == 0 {
                    return Ok(None);
                }
                let count = if full_key {
                    let mut key = vec![value.clone()];
                    key.extend(suffix_values.iter().cloned());
                    index
                        .get(reader, &key)
                        .await?
                        .count_up_to(*remaining)
                        .await?
                } else if fields > 1 {
                    index
                        .scan_prefix(reader, std::slice::from_ref(value), false)
                        .await?
                        .count_up_to(*remaining)
                        .await?
                } else {
                    index
                        .get(reader, std::slice::from_ref(value))
                        .await?
                        .count_up_to(*remaining)
                        .await?
                };
                let Some(count) = consume_budget(count, remaining) else {
                    return Ok(None);
                };
                total += count;
            }
            return Ok(Some(total));
        }
        IndexScanType::PrefixScan { prefix_values, .. } => {
            index
                .scan_prefix(reader, prefix_values, false)
                .await?
                .count_up_to(*remaining)
                .await?
        }
        IndexScanType::RangeScan {
            prefix_values,
            lower,
            upper,
            ..
        } => {
            index
                .scan_range(reader, prefix_values, lower.clone(), upper.clone(), false)
                .await?
                .count_up_to(*remaining)
                .await?
        }
        IndexScanType::OrScan { branches } => {
            let mut total = 0;
            for branch in branches {
                let Some(count) = Box::pin(count_scan(index, reader, branch, remaining)).await?
                else {
                    return Ok(None);
                };
                total += count;
            }
            return Ok(Some(total));
        }
        _ => {
            *remaining = 0;
            return Ok(None);
        }
    };
    Ok(consume_budget(count, remaining))
}

fn consume_budget(count: usize, remaining: &mut usize) -> Option<usize> {
    if count == *remaining {
        return None;
    }
    // Empty probes still cost an index lookup and must consume budget.
    *remaining -= count.max(1);
    Some(count)
}
