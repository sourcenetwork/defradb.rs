use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query, db, storage, document";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #3245: `_all` over an empty array is true, but an array index holds
        // an empty array only under its null entry, so the element scan alone
        // missed it; the scan also reads the null entry, except when it reads
        // it already or an `_any` on the field rules empty arrays out.
        family: "Index selection",
        name: "AllEmptyArray.widened_finds_all / element_scan_misses_empty / widened_adds_only_null / widened_same_when_null_read / any_scan_finds_all_and_any",
        axis: Lean,
        anchor: "crates/engine/query/src/planner/index_selection/empty_array.rs with_empty_arrays, visits_null; crates/database/db/src/index/manager/value_extraction.rs expand_value_for_indexing",
        model_ref: "AllEmptyArray (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/index/all_empty_array.rs all_on_an_indexed_array_finds_an_empty_array, all_on_an_indexed_composite_array_finds_an_empty_array, all_with_a_range_on_an_indexed_array_finds_an_empty_array, all_with_a_range_on_a_descending_indexed_array_finds_an_empty_array; crates/engine/query/src/planner/index_selection/empty_array_tests.rs all_lt_on_an_ascending_index_reads_the_null_entry_once, all_lt_on_a_descending_index_reads_the_null_entry_once, all_gt_on_a_descending_index_also_reads_the_null_entry, any_beside_all_reads_only_its_elements",
    },
    Property {
        // The JSON half of #3245: a JSON index wrote no entry for an empty
        // array, so `_all` at its path missed it. The scan pieces are private
        // to the planner and the range iterator. Boundary.
        family: "Index selection",
        name: "AllScanEmptyArray.all_scan_complete / all_scan_sound / value_scan_skips_empty / neg_scan_finds_empty / any_scan_complete / empty_arrays_never_clash / same_path_scan_complete / pinned_scan_complete / narrowed_scan_complete / chosen_scan_pins / chosen_scan_complete / entry_scan_complete / separate_any_complete / clause_scan_complete: `_all` with a positive predicate at a JSON path finds every matching document, empty arrays included, a value scan returns no empty array while a bare negative scan reads the marker and finds them, a scan tests a `_like` pattern only at its own path and `_any` clause, a full scan reads the path of a condition a missing path fails and never of a bare negative at a path, and bounds from two `_any` clauses are never joined",
        axis: Lean,
        anchor: "crates/building-blocks/document/src/normal.rs extract_json_leaves; crates/engine/query/src/planner/index_selection/filter_to_scan.rs filter_to_index_scan, suffix_eq_values, at_entry_path; crates/engine/query/src/planner/index_selection/empty_array.rs all_path, with_json_empty_arrays; crates/engine/query/src/planner/index_selection/conditions.rs extract_field_conditions, is_full_scan, accepts_missing, should_fallback_to_full_scan, looks_for_null; crates/engine/query/src/planner/index_selection/types.rs FieldCondition::clause, FieldCondition::nested_array_op, FieldCondition::non_scalar_member; crates/engine/storage/src/index/range_iterator.rs RangeIterator::next; crates/engine/storage/src/index/mod.rs skips_uniqueness",
        model_ref: "AllScanEmptyArray (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/index/empty_array.rs; tools/integration-test/tests/testo/index/json_range_paths.rs; tools/integration-test/tests/testo/index/json_index_matches_scan.rs; crates/engine/storage/src/index/range_iterator_tests.rs a_range_reads_one_empty_array_entry_not_each",
    },
];
