//! Equal index keys come back in index order, as Go returns them (#2712).
//!
//! An index key ends with the document's node-local short ID, so the entries
//! of one indexed value scan in the order their documents were first stored
//! on this node: created here, or first received from a peer. Two replicas
//! that received the same documents in a different order list them
//! differently. Go keys its index the same way (`getDocumentsIndexKey`,
//! `internal/db/index.go`) and returns that order.

use rapidhash::RapidHashSet;

use query::planner::index_selection::IndexScanType;
use storage::index::IndexEntry;

/// Append one InScan value's entries in index order, skipping short IDs
/// already seen.
pub(crate) fn extend_unique(
    all: &mut Vec<u64>,
    seen: &mut RapidHashSet<u64>,
    entries: impl IntoIterator<Item = IndexEntry>,
) {
    for entry in entries {
        if seen.insert(entry.doc_short_id) {
            all.push(entry.doc_short_id);
        }
    }
}

/// Apply an ExactMatch's offset and limit to its DocIDs, which are in index
/// order. The scan collects the whole equal-key set first so its
/// `indexFetches` counts every entry; other scan types page as they scan.
pub(crate) fn apply_exact_match_page(
    scan_type: &IndexScanType,
    doc_ids: &mut Vec<String>,
    offset: u64,
    limit: Option<u64>,
) {
    if !matches!(scan_type, IndexScanType::ExactMatch { .. }) {
        return;
    }
    let start = (offset as usize).min(doc_ids.len());
    let end = match limit {
        Some(limit) => start.saturating_add(limit as usize).min(doc_ids.len()),
        None => doc_ids.len(),
    };
    doc_ids.truncate(end);
    doc_ids.drain(..start);
}

#[cfg(test)]
mod tests {
    use super::*;
    use document::NormalValue;
    use rapidhash::HashSetExt;
    use storage::index::Bound;

    fn exact() -> IndexScanType {
        IndexScanType::ExactMatch { values: vec![] }
    }

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn exact_match_keeps_index_order() {
        let mut got = ids(&["b", "a"]);
        apply_exact_match_page(&exact(), &mut got, 0, None);
        assert_eq!(got, ["b", "a"]);
    }

    #[test]
    fn exact_match_limit_one_picks_the_first_in_index_order() {
        let mut got = ids(&["b", "a", "c"]);
        apply_exact_match_page(&exact(), &mut got, 0, Some(1));
        assert_eq!(got, ["b"]);
    }

    #[test]
    fn exact_match_offset_and_limit_page_in_index_order() {
        let mut got = ids(&["c", "a", "b", "d"]);
        apply_exact_match_page(&exact(), &mut got, 1, Some(2));
        assert_eq!(got, ["a", "b"]);
    }

    #[test]
    fn exact_match_offset_past_end_is_empty() {
        let mut got = ids(&["b", "a"]);
        apply_exact_match_page(&exact(), &mut got, 5, None);
        assert!(got.is_empty());
    }

    #[test]
    fn other_scans_are_left_as_scanned() {
        let mut got = ids(&["b", "a"]);
        let range = IndexScanType::RangeScan {
            prefix_values: vec![],
            lower: Bound::Unbounded,
            upper: Bound::Unbounded,
            reverse: false,
        };
        apply_exact_match_page(&range, &mut got, 1, Some(1));
        assert_eq!(got, ["b", "a"]);
    }

    fn entry(short_id: u64, value: i64) -> IndexEntry {
        IndexEntry::new(short_id, vec![NormalValue::Int(value)])
    }

    #[test]
    fn in_scan_entries_keep_index_order_without_duplicates() {
        let mut all = Vec::new();
        let mut seen = RapidHashSet::new();
        extend_unique(&mut all, &mut seen, vec![entry(3, 7), entry(1, 7)]);
        extend_unique(&mut all, &mut seen, vec![entry(1, 8), entry(2, 8)]);
        assert_eq!(all, [3, 1, 2]);
    }
}
