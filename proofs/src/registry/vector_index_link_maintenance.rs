use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "storage, db-index";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Vector index link maintenance",
        name: "INV_NoLostLinks — blind additions preserve links and stale structural pruning aborts",
        axis: Tla,
        anchor: "crates/engine/storage/src/backends/regolith/merge/vector.rs; crates/database/db/src/index/vector/engine/hnsw/insert.rs; crates/database/db/tests/vector/concurrent_hnsw.rs",
        model_ref: "MC_VectorLinks_Green.cfg / MC_VectorLinks_Red_Puts.cfg / MC_VectorLinks_Red_Prune.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/concurrent_hnsw.rs overlapping_inserts_keep_shared_links_and_metadata_counts, structural_pruning_conflicts_with_a_new_link",
    },
];
