use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The pinc endpoint holds each request's `!Send` stream under the
        // `Send` id its token carries. A reply or a dropped token takes the
        // entry; a drop lost to a full queue is caught by the deadline sweep.
        family: "pinc reply bridge",
        name: "INV_OneOutcome / INV_Bounded / INV_NoWriteAfterEnd / LIVE_NoLeak / LIVE_DropResets: a stream gets one answer, the table stays bounded and empties for good, a dropped token's stream is reset",
        axis: Tla,
        anchor: "crates/database/p2p/src/pinc/token.rs PincResponseToken; crates/database/p2p/src/pinc/reply_table.rs ReplyTable::issue, ReplyTable::take, ReplyTable::purge, ReplyTable::sweep; crates/database/p2p/src/pinc/endpoint.rs Node::handle, Node::sweep_loop, Node::disconnected, REPLY_DEADLINE",
        model_ref: "MC_PincReplyBridge_{Green,Green_NoLoss,Red_NoTakeOnce,Red_NoDropReset,Red_NoSweep,Red_NoCap}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/pinc/tests/bridge.rs a_reply_after_the_connection_ended_is_an_error_not_a_write, many_dropped_tokens_leave_no_stream_behind; crates/database/p2p/src/pinc/tests/mux.rs a_reset_stream_does_not_tear_down_the_connection",
    },
    Property {
        family: "pinc reply bridge",
        name: "full_refuses / issue_bounded / issue_fresh / take_once / take_only / purge_only / sweep_only: the reply table refuses past its bound, never reuses an id, and takes each entry once",
        axis: Lean,
        anchor: "crates/database/p2p/src/pinc/reply_table.rs ReplyTable::issue, ReplyTable::take, ReplyTable::purge, ReplyTable::sweep",
        model_ref: "PincReplyTable.Table (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/pinc/reply_table.rs a_stream_is_taken_at_most_once, ids_are_never_reused, a_full_table_hands_the_stream_back, purge_takes_only_the_ended_connection, sweep_takes_only_what_is_due",
    },
];
