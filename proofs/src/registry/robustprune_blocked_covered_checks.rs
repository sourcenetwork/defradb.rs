use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "RobustPrune blocked covered checks",
        name: "blocked_eq_sequential — checking candidates in blocks keeps exactly the edges the one-at-a-time walk keeps, for any cut into blocks",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/select/robust_prune.rs RobustPrune::select; crates/foundation/defra-core/src/vector/kernel/mod.rs squared_euclidean_block",
        model_ref: "RobustPruneBlocks.blocked_eq_sequential / no_since_keeps_a_covered_edge",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/edge_selection.rs robust_prune_keeps_the_edges_one_at_a_time_checking_keeps",
    },
];
