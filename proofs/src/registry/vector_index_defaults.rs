use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A vector index description's stored form is the normalized one:
        // Go's defaults filled in, and an empty algorithm or metric read as
        // the default. Normalizing is stable, so the stored form never drifts.
        family: "Vector index defaults",
        name: "normalize_idempotent / normalize_hnsw_fills / normalize_other_unchanged / empty_reads_default / nonempty_reads_itself: normalizing twice is normalizing once, an HNSW index is stored with every parameter set, any other index is untouched, and an empty string reads as the default",
        axis: Lean,
        anchor: "crates/foundation/schema/src/vector_index_defaults.rs VectorIndexDescription::normalized, empty_is_default",
        model_ref: "VectorIndexDefaults (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
