use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "storage, events, db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Receipt-ordered event dispatch",
        name: "Ordered / INV_CommitOrder / INV_CommittedEventsKept / INV_HooksBeforeEvents / INV_OnlyCommitted — committed events go out in commit-receipt order, a write-free commit no earlier than what already went out, after their commit's hooks, never for an aborted commit and never lost; receipts that come back reversed to the hand-overs still go out in receipt order",
        axis: Tla,
        anchor: "crates/engine/storage/src/backends/regolith/event_order.rs EventOrder::bind, EventOrder::hand_over, EventOrder::hooks_done, Binding::hand_over, HooksRunning::done, Drainer::apply_life, Drainer::apply_work, Drainer::complete, Drainer::floor, Drainer::dispatch; crates/database/db/src/event/emission.rs register_ordered_events",
        model_ref: "MC_EventOrder_Green.cfg / MC_EventOrder_Red_{Arrival,ReceiptRegress,NoFloorClamp,DispatchBeforeAttach,DispatchDuringHooks,AbortPublishes,NotMinSeq}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/storage/tests/ordered_event_receipt.rs eventful_empty_commit_commits_and_dispatches_its_events, accepted_identical_blind_write_keeps_its_event_callback, failed_ordered_hook_keeps_the_commit_its_events_and_later_commits; crates/engine/storage/src/backends/regolith/event_order.rs reversed_concurrent_receipts_dispatch_in_seq_order, a_later_receipt_waits_for_an_earlier_commit_still_in_flight, an_aborted_commit_releases_the_stream_and_sends_nothing, hooks_hold_their_commit_and_everything_after_it, a_write_free_commit_goes_out_no_earlier_than_what_already_went_out",
    },
];
