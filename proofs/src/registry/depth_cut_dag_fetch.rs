use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Depth-cut DAG fetch",
        name: "INV_HistoryNotResent / INV_RoundsLogarithmic / LIVE_Completes",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/car.rs collect_dag_blocks_within; crates/database/p2p/src/sync/coordinator/closure_depth.rs ClosureDepth::widen; crates/database/p2p/src/sync/coordinator/dag_fetcher.rs fetch_dag_attempt",
        model_ref: "MC_DepthCutFetch_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Depth-cut DAG fetch",
        name: "within_iff_level / deep_block_not_served / within_le / cut_le_cap / cut_ge_pow / cut_reaches",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/car.rs collect_dag_blocks_within; crates/database/p2p/src/sync/coordinator/closure_depth.rs ClosureDepth::initial, ClosureDepth::widen",
        model_ref: "CarClosureDepth (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // libp2p CAR bodies have one fixed total read deadline. A response
        // slower than CAR_MAX_BYTES over that deadline fails, and the fetch
        // falls back to Bitswap.
        family: "Depth-cut DAG fetch",
        name: "LIVE_Completes on libp2p assumes a CAR body arrives within the stream read deadline",
        axis: Tla,
        anchor: "crates/database/p2p/src/two_stream/handler/car.rs read_stream; crates/database/p2p/src/two_stream/runner.rs TwoStreamRunner::new",
        model_ref: "MC_DepthCutFetch_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
