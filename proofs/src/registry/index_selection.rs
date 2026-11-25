use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query, db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #3245: `_all` over an empty array is true, but an array index holds
        // an empty array only under its null entry, so the element scan alone
        // missed it; the scan also reads the null entry.
        family: "Index selection",
        name: "AllEmptyArray.widened_finds_all / element_scan_misses_empty / widened_adds_only_null",
        axis: Lean,
        anchor: "crates/engine/query/src/planner/index_selection/empty_array.rs with_empty_arrays; crates/database/db/src/index/manager/value_extraction.rs expand_value_for_indexing",
        model_ref: "AllEmptyArray (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/index/all_empty_array.rs all_on_an_indexed_array_finds_an_empty_array, all_on_an_indexed_composite_array_finds_an_empty_array, all_with_a_range_on_an_indexed_array_finds_an_empty_array",
    },
];
