use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // No test binds graph.rs walk to WalkCutoff.
        family: "DiskANN walk cutoff",
        name: "cut_step_eq_step — skipping neighbours ranked after a full frontier's last place leaves the frontier the full step would",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/graph.rs walk, each_edge, by_distance",
        model_ref: "WalkCutoff.cut_step_eq_step / distance_only_cutoff_drops_a_tie",
        tiers: &[Boundary],
        bound_by: "",
    },
];
