use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db-index";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The flat code table is a pure in-memory structure loaded once per
        // graph generation; its laws are proven, and the unit tests in
        // crates/database/db/tests/vector/diskann_resident.rs exercise the Rust side.
        family: "DiskANN resident code table",
        name: "get_returns_inserted / get_in_bounds / insert_refuses_other_length: each slot reads back its own code, in bounds",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/cache.rs Codes::insert, Codes::get",
        model_ref: "DiskAnnCodes (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
