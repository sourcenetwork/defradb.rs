use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Lean marker values (mNull=0..mIntMax=253) verified to match the live
        // `crates/engine/storage/src/encoding/mod.rs` constants by inspection, but those
        // are `pub(crate)` (no static assert without widening prod visibility) and
        // a plain ordered query may sort in memory rather than exercise the index
        // key encoding — so neither tier is cleanly realizable. Boundary.
        family: "Order-preserving key encoding",
        name: "asc_strictly_order_preserving_* — encoding is a strict-order embedding",
        axis: Lean,
        anchor: "crates/engine/storage/src/encoding/mod.rs type-marker constants",
        model_ref: "OrderEncoding (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // #3245: a JSON index writes a marker under each empty array so `_all`
        // finds it; the marker must start like no value, keep the values'
        // order, stay inside its path and read back as itself. The unit tests
        // pin the marker bytes the model uses.
        family: "Order-preserving key encoding",
        name: "JsonEmptyArray.marker_distinct_* / value_lt_marker_asc / marker_lt_value_desc / marker_inside_path_* / decode_marker_* / decode_value_* — the JSON empty-array marker",
        axis: Lean,
        anchor: "crates/engine/storage/src/encoding/json.rs JSON_EMPTY_ARRAY, JSON_EMPTY_ARRAY_DESC, encode_scalar_ascending, encode_scalar_descending, decode_scalar_ascending, decode_scalar_descending, decode_json_ascending, decode_json_descending; crates/building-blocks/document/src/json_leaf.rs json_index_leaves",
        model_ref: "OrderEncoding.JsonEmptyArray (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/engine/storage/src/encoding/json_empty_array_tests.rs the_markers_match_the_lean_model, the_marker_round_trips_both_ways, the_marker_sorts_after_every_value_ascending_and_before_descending, the_marker_stays_inside_its_path, every_value_still_decodes_as_itself, nested_array_paths_round_trip_descending, names_that_look_like_an_index_round_trip; tools/integration-test/tests/testo/index/json_all_empty_array.rs",
    },
    Property {
        family: "Order-preserving key encoding",
        name: "JsonPathMarker.PropertyIsNotIndex / IndexIsIndex",
        axis: Tla,
        anchor: "crates/engine/storage/src/encoding/json.rs decode_json_ascending, decode_json_descending",
        model_ref: "MC_JsonPathMarker_{Green,Red_Range}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/storage/src/encoding/json_array_path_tests.rs descending_json_unicode_property_and_array_path_round_trip",
    },
    Property {
        family: "Order-preserving key encoding",
        name: "descending_json_sentinels_bracket_values / utf8_lead_is_not_array_marker",
        axis: Lean,
        anchor: "crates/engine/storage/src/encoding/json.rs encode_json_descending, decode_json_descending",
        model_ref: "OrderEncoding.JsonPath (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/storage/src/encoding/json_array_path_tests.rs descending_json_sentinels_bracket_values, descending_json_unicode_property_and_array_path_round_trip",
    },
    Property {
        // `order_in_values` is private to the planner, so no contract test
        // reaches it; crates/entry-points/defra-node/tests/in_filter_order.rs runs the
        // queries it orders over an ASC, a DESC and no index, and the testo
        // scenarios in index/composite_in_order.rs the unordered ones. Boundary.
        family: "Order-preserving key encoding",
        name: "lookups_strictly_increasing / reversed_lookups_strictly_decreasing / written_sublist — `_in` point lookups run in key order under an order, as written without one, each once",
        axis: Lean,
        anchor: "crates/engine/query/src/planner/index_selection/filter_to_scan.rs filter_to_index_scan; crates/engine/query/src/planner/index_selection/path_bounds.rs order_in_values",
        model_ref: "OrderEncoding.InLookup (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
