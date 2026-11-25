use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Pull sync (doc-sync / sync-branchable)",
        name: "Delivered / OneReplyEach / RefusedOnlyUnsent / NoResendAfterDeadline — a pubsub_rpc request is held until a peer subscribes, resent to each later subscriber, and never resent after its deadline",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/held_request.rs HeldRequest::next, HeldRequest::publish, HeldRequest::publish_by_deadline, HeldRequest::finish; crates/database/p2p/src/sync/coordinator/pubsub_services.rs PubsubServices::peer_subscribed",
        model_ref: "MC_HeldPubsubRequest_{Green,Red_NoHold,Red_NoResend,Red_LateWatch,Red_NoDedupe,Red_ResendLate}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/held_request_tests.rs doc_sync_sent_before_the_topic_has_a_peer_waits_for_one",
    },
];
