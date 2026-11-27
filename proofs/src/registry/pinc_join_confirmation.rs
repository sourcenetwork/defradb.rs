use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A pinc join returns before a hub knows of it, so a subscribe
        // answers only once a hub confirmed the topic's join, and fails
        // after JOIN_WAIT with none, taking the topic back so the retry
        // joins afresh. GREEN assumes pinc confirms only the subscription
        // that asked (pinc #483); Red_UntaggedList and Red_Undrained are
        // pinc as it is today.
        family: "pinc join confirmation",
        name: "INV_OkOnlyWhenHeld / INV_FailOnlyUnconfirmed / INV_NoMissedWake / INV_NoStuckSubscription / INV_JoinedMatches: a subscribe answers only while a hub holds its join, never fails once confirmed, never waits for an event already read, a lost event never strands a subscription, and pinc is joined exactly while the engine or a hold has the topic",
        axis: Tla,
        anchor: "crates/database/p2p/src/pinc/topics.rs Node::subscribe, Node::answer_when_confirmed, Node::joined, Node::abandon_join, Node::leave, Node::unsubscribe, Node::hold_fanout, Node::expire_fanout, JOIN_WAIT",
        model_ref: "MC_PincJoinConfirm_{Green,Green_Wide,Red_NoConfirmCheck,Red_NoConfirmCheck_Wake,Red_NoStillJoined,Red_NoRecheck,Red_NoRollback,Red_NoClearOnLeave,Red_UntaggedList,Red_Undrained}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/pinc/tests/topics.rs a_subscribe_no_hub_confirms_fails_and_its_retry_waits_again, connected_subscribers_see_each_other_join_and_leave",
    },
];
