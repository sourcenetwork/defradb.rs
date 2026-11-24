use super::{Axis::*, Property, Tier::*};

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
];
