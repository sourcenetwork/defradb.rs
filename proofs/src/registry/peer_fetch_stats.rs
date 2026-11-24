use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Peer fetch stats",
        name: "roll_ratio_stays_a_ratio / rolling_over_samples / bump_never_overflows",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/fetch_stats.rs PeerCounters::record, roll",
        model_ref: "FetchStats (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
