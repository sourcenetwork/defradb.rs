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
        anchor: "crates/engine/query/src/planner/index_selection/filter_to_scan.rs filter_to_index_scan, order_in_values",
        model_ref: "OrderEncoding.InLookup (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The marker constants are `pub(crate)`; the storage unit tests check
        // they differ from null and from each other, and that an empty array
        // round-trips between its path's sentinels. Boundary.
        family: "Order-preserving key encoding",
        name: "markers_empty_array_band / markers_empty_array_fresh / cross_type_empty_array / empty_array_desc_not_flipped / descending_empty_array_bracketed / ascending_empty_array_bracketed: the empty array markers are free bytes below the integers, the descending one not the flipped ascending one, and a JSON empty array entry sits between its path's sentinels",
        axis: Lean,
        anchor: "crates/engine/storage/src/encoding/mod.rs EMPTY_ARRAY, EMPTY_ARRAY_DESC; crates/engine/storage/src/encoding/empty_array.rs encode_empty_array_ascending, encode_empty_array_descending; crates/engine/storage/src/encoding/json.rs encode_scalar_ascending, encode_scalar_descending",
        model_ref: "OrderEncoding.Markers / OrderEncoding.CrossType / OrderEncoding.JsonPath (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/storage/src/encoding/empty_array.rs empty_array_is_not_null; crates/engine/storage/src/encoding/json_array_path_tests.rs an_empty_array_round_trips_between_its_path_sentinels",
    },
];
