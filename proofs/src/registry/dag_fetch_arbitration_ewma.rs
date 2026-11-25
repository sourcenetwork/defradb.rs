use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "DAG fetch arbitration (EWMA)",
        name: "ArbitrationConstants: VALUE_BITS, TIME_BITS, WEIGHT_ONE, the half-lives, shifts, tick, TIE_BUCKET, BACKOFF_FLOOR, units, floor and WEIGHT_TABLE, with table_is_floor / table_decreasing / table_wraps / packed_word_fits / tie_bucket_le_backoff_floor / useful_floor_is_one_64th / occupancy_half_life_ticks checked on exactly these values",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/arbitration/mod.rs VALUE_BITS, TIME_BITS, WEIGHT_ONE, WEIGHT_TABLE, PROVIDER_HALF_LIFE; crates/database/p2p/src/sync/arbitration/ewma.rs weight_q32; proofs/tests/lean_conformance.rs lean_arbitration_constants_match_rust",
        model_ref: "Arbitration.Constants (lake build); Conformance.lean ArbitrationConstants",
        tiers: &[Contract],
        bound_by: "proofs/tests/lean_conformance.rs lean_arbitration_constants_match_rust",
    },
    Property {
        family: "DAG fetch arbitration (EWMA)",
        name: "roll_le_bound / troll_le_bound / record_first / troll_full / iter_monotone_up / iter_stays_below / decay_geometric / troll_decay_up / troll_decay_down / dither_mean / weight_le_one / weight_zero_gap / weight_first_ms / expected_useful_le / bucket_order_any_score / inversion_within_bucket / rearmed_root_goes_behind",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/arbitration/ewma.rs roll, troll, record_word, PackedEwma; root.rs RootEstimates::observe; occupancy.rs advance; provider.rs ProviderEstimates, expected_useful; due_order.rs tie_break_order; tests.rs; tests/unit/estimator_trace_replay.rs; tests/unit/estimators_observe_only.rs",
        model_ref: "Arbitration (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
