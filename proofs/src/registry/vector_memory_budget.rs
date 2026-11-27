use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db-index (vector)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Vector memory budget",
        name: "INV_BudgetHolds / INV_Accounted — the vector caches of a node never hold more than its budget, and the budget's counter is exactly what they hold",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/budget.rs MemoryBudget, Charge::grow, Charge::release; crates/database/db/src/index/vector/engine/diskann/reload.rs Reload::begin, Reload::insert, Reload::finish; crates/database/db/src/index/vector/engine/diskann/cache.rs CodeCache::insert, CodeCache::replace_charged",
        model_ref: "MC_VectorMemoryBudget_Green.cfg (GREEN) / MC_VectorMemoryBudget_Red_SplitCheck.cfg / MC_VectorMemoryBudget_Red_Leak.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_memory_budget.rs concurrent_charges_never_pass_the_limit, a_search_returns_the_same_hits_under_any_budget, a_reload_taken_over_does_not_publish",
    },
    Property {
        family: "Vector memory budget",
        name: "INV_FullLoad — a reload publishes every code that fits in the budget: the codes it replaces, missed codes and pending copies never take its room",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/engine/diskann/reload.rs Reload::begin, Reload::stop; crates/database/db/src/index/vector/engine/diskann/cache.rs CodeCache::insert, CodeCache::publish_pending; crates/database/db/src/index/vector/engine/diskann/pending_cache.rs PendingCache::clear; crates/database/db/src/index/vector/engine/diskann/graph.rs refresh_codes",
        model_ref: "MC_VectorMemoryBudget_Green.cfg (GREEN) / MC_VectorMemoryBudget_Red_KeepOld.cfg / MC_VectorMemoryBudget_Red_MissesWhileLoading.cfg / MC_VectorMemoryBudget_Red_PendingWhileLoading.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_memory_budget.rs a_reload_gets_the_room_the_old_codes_held, a_miss_is_kept_only_with_room_and_no_reload_running",
    },
    Property {
        family: "Vector memory budget",
        name: "INV_ClaimLostOnlyToNewer / INV_NoTakeAfterLoss / INV_NoClaimClearedByStaleAbandon — only one search reloads a generation, a reload loses its claim only to a newer generation, and a stale reload neither takes a later code nor clears the newer claim",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/engine/diskann/reload.rs Reload::begin, Reload::owned, Reload::finish; crates/database/db/src/index/vector/engine/diskann/cache.rs Claim, CodeCache::next_ticket, CodeCache::replace_charged",
        model_ref: "MC_VectorMemoryBudget_Green.cfg (GREEN) / MC_VectorMemoryBudget_Red_NoSingleFlight.cfg / MC_VectorMemoryBudget_Red_StaleFinish.cfg / MC_VectorMemoryBudget_Red_StepAfterLoss.cfg / MC_VectorMemoryBudget_Red_StaleAbandon.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_memory_budget.rs one_search_reloads_a_generation_at_a_time, a_reload_taken_over_does_not_publish",
    },
    Property {
        family: "Vector memory budget",
        name: "take_within_limit / take_conserves / give_all_restores / replace_within_limit / replace_conserves / cover_reaches / settle_exact — a take or a charge replacement never passes the limit, the counter always adds up to the charges, and a cover or settle holds what it promises",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/budget.rs MemoryBudget::take, MemoryBudget::give, Charge::cover, Charge::settle, Charge::release",
        model_ref: "VectorMemoryBudget.take_within_limit / take_conserves / take_refused / give_conserves / give_all_restores / replace_within_limit / replace_conserves / cover_reaches / settle_exact / split_check_overshoots",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_memory_budget.rs a_charge_past_the_limit_is_refused_and_a_dropped_one_given_back, settling_gives_back_the_excess_or_takes_what_is_missing, concurrent_charges_never_pass_the_limit",
    },
];
