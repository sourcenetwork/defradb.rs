use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "DAG fetch provider ranking",
        name: "INV_WaitBounded — one attempt waits at most a rotation of stalls plus its free refusals, however many batches",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/dag_fetcher.rs fetch_dag_attempt, poll_fetch_blocks_rotating, window_outcome; crates/database/p2p/src/sync/coordinator/dag_retry.rs FAST_REFUSAL, MAX_FREE_REFUSALS",
        model_ref: "MC_DagFetchBudget_{Green,Red_SlowIsRefusal,Red_Uncapped,Red_NoCheck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "DAG fetch provider ranking",
        name: "rank_perm / rank_keeps_healthy_source_first / rank_all_healthy / next_delay_in_range / stale_starts_over / forget_keeps_fresh / forget_drops_stale",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/coordinator/provider_health.rs ProviderHealth::rank, record_fast_failure, forget_stale, Cooldown::is_stale",
        model_ref: "ProviderHealth (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
