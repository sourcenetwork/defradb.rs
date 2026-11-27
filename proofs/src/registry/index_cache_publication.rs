use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db-index, storage";

pub(super) const PROPERTIES: &[Property] = &[Property {
    // Index changes and backfill settlements reach the collection cache the
    // planner reads: a create or drop returns only after its publication
    // ran, a backfill's settling commit publishes the index itself, and a
    // publication applies only its own index change, so the cache ends where
    // the store does.
    family: "Index cache publication",
    name: "INV_ReturnSeesOwn / INV_CacheMatchesStore",
    axis: Tla,
    anchor: "crates/database/db/src/index/settle.rs FinishIndexAttempt, FailBackfillAttempt, publish_on_commit; crates/database/db/src/index/create.rs DB::define_index, DB::allocate_index_id, DB::stage_index_definition; crates/database/db/src/index/manager/mod.rs IndexManager::create_index_of_kind_with_id; crates/database/db/src/index/drop.rs DB::stage_index_drop; crates/database/db/src/index/backfill.rs DB::backfill_index; crates/database/db/src/collection/mod.rs Collection::load_index_actions, Collection::with_current_index_actions; crates/database/db/src/collection/epoch.rs EpochedCollections::publish, EpochedCollections::publish_statuses, EpochedCollections::rcu_if_epoch; crates/engine/storage/src/corekv/callbacks.rs OnCommit::OrderedAndWait",
    model_ref: "MC_IndexCachePublication_{Green,Red_Queue,Red_QueueReload,Red_Reload,Red_Snapshot}.cfg",
    tiers: &[Contract],
    bound_by: "crates/database/db/tests/index/publication.rs an_index_created_while_a_commit_hook_holds_events_is_used, an_index_dropped_while_a_commit_hook_holds_events_can_be_created_again",
}];
