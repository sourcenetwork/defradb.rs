use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Calls a C caller makes on one explicit transaction run one at a
        // time, in call order, even when a call returns before its work runs
        // (the asynchronous query): each takes a ticket as it names the
        // transaction, and a ticket dropped before its turn is skipped once.
        family: "FFI transaction turns",
        name: "INV_OneRunning / INV_NoOvertake / Progress: calls on a transaction run one at a time, none overtakes an earlier ticket (a commit never lands before work asked for first), and no call waits forever on a dropped ticket",
        axis: Tla,
        anchor: "crates/entry-points/ffi/src/state/turns.rs Turns::take, Turns::finish, Ticket::wait, Ticket; crates/entry-points/ffi/src/target.rs resolve_target, TxnCall; crates/entry-points/ffi/src/request.rs Request::resolve; crates/entry-points/ffi/src/query/exec.rs Prepared::run, run_in_turn, defra_query_execute_async; crates/entry-points/ffi/src/txn/lifecycle.rs defra_transaction_commit, defra_transaction_discard",
        model_ref: "MC_TxnTurns_{Green,Red_NoRecheck,Red_CancelSkipsNow}.cfg",
        tiers: &[Contract],
        bound_by: "crates/entry-points/ffi/src/state/turns_tests.rs tickets_are_served_in_the_order_they_were_taken, a_ticket_dropped_before_its_turn_is_skipped, a_ticket_dropped_on_its_turn_serves_the_next, calls_from_several_threads_never_run_at_once; crates/entry-points/ffi/src/call/tests.rs an_async_query_in_a_transaction_returns_before_it_runs, calls_on_a_transaction_run_in_the_order_they_were_made",
    },
    Property {
        family: "FFI transaction turns",
        name: "take_hands_out_next / take_moves_next / skip_passes_only_dropped: tickets come out distinct and in call order, and skipping passes only dropped tickets",
        axis: Lean,
        anchor: "crates/entry-points/ffi/src/state/turns.rs Turns::take, Turns::finish",
        model_ref: "TxnTurns (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
