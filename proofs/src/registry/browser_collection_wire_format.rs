use super::{Axis::*, Property, Tier::*};

pub(super) const CRATES: &str = "wasm";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Browser collection wire format",
        name: "SelectedBlockOnly / DefaultIsNumber",
        axis: Tla,
        anchor: "crates/surfaces/wasm/src/collection_js.rs IndexJs::from, collections_to_js",
        model_ref: "CollectionWire.tla",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Browser collection wire format",
        name: "selected_kept / unselected_absent / same_integer_token",
        axis: Lean,
        anchor: "crates/surfaces/wasm/src/collection_js.rs IndexJs::from, collections_to_js",
        model_ref: "CollectionWire (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
