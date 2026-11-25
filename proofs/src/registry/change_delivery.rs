use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "events, db, db-merge, query, http";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Change delivery",
        name: "INV_ExactlyOnce / INV_CommitOrder / INV_HiddenNotShown / INV_LossCounted / EventuallyObserved — a subscriber that subscribes, then reads the update journal from its saved cursor on every wake-up and loss signal, observes every committed update of the collection, local or merged, that it is allowed to see, exactly once, in commit order, across a restart, with no rescan; a row hidden from it (a deleted or purged document, or one it may not read, in a collection with a policy; a chunk whose parent it may not read) is never shown and its cursor is passed, so it is skipped for good; every drop is counted. Not prevented: the raw subscription's counted drop wakes no reader (Red_CounterNoWake), so it is no wake-up source for the pattern",
        axis: Tla,
        anchor: "crates/database/db/src/event/updates.rs record, read, once_key; crates/database/db/src/event/updates/gate.rs Gate::admits; crates/database/db/src/merge/merge_handler/composite_persist.rs persist_merged_document; crates/foundation/events/src/channel_bus.rs ChannelBus::publish_raw, Subscriber::record_overflow; crates/foundation/events/src/document_changes.rs ChangePublisher::publish; crates/engine/query/src/runner/arrivals.rs execute_arrivals_query",
        model_ref: "MC_ChangeDelivery_{Green_Closing,Green_Coalescing}.cfg / MC_ChangeDelivery_Red_{SilentLoss,ReadBeforeSubscribe,JournalSkipsMerge,ReMergeAppends,CounterNoWake,WakeBeforeState,CursorNotDurable,GateOff,HiddenStalls}.cfg",
        tiers: &[Behavioral],
        bound_by: "tools/integration-test/tests/testo/subscription/delivery/resume.rs a_saved_cursor_resumes_after_a_reopen_in_commit_order; tools/integration-test/tests/testo/subscription/delivery/merge_once.rs a_replicated_update_appears_once; tools/integration-test/tests/testo/subscription/delivery/beyond_head.rs a_cursor_past_the_head_is_refused; tools/integration-test/tests/testo/subscription/delivery/visibility.rs hidden_rows_are_passed_and_not_shown, a_collection_without_a_policy_shows_a_deleted_documents_rows; crates/database/db/tests/read/update_visibility.rs a_deleted_private_documents_updates_are_shown_to_nobody; crates/database/db/tests/merge/update_journal.rs a_block_merged_again_by_another_handler_adds_no_second_row; crates/surfaces/http/tests/graphql_sse_recovery.rs overflow_ends_the_graphql_stream_before_a_queued_result; crates/foundation/events/tests/unit/channel_bus.rs overflow_wakes_reader_that_drained_before_loss_was_recorded, ordinary_raw_subscription_remains_open_after_overflow, raw_overflow_recorded_after_a_drain_is_counted_but_wakes_no_reader; crates/foundation/events/tests/document_changes.rs distinct_document_overflow_is_one_bounded_resync_then_recovers, publishing_races_with_draining_without_losing_a_wake",
    },
];
