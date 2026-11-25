use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Gossip announcement audience",
        name: "INV_GoAudience / LIVE_Announced: a write committed with no peer subscribed is announced at once and never published again, so it reaches only peers subscribed at its commit, as Go's SendUpdate; every write is announced",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/broadcast.rs broadcast_local_update_with_creator; crates/database/p2p/src/sync/peer_state/tracker/operations.rs has_subscriber; crates/database/p2p/src/sync/broadcast_coalescer.rs BroadcastCoalescer::run; crates/database/db/src/merge/broadcast_mutator/broadcast.rs broadcast_with_creator",
        model_ref: "MC_AnnounceAudience_{Green,Red_HoldWhenAlone,Red_AloneMeansDisconnected,Red_RetryWhileConnected}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/broadcast_pool_tests.rs a_write_with_no_peer_connected_is_not_held_for_a_peer_that_joins, a_write_with_no_peer_subscribed_is_not_held_for_a_peer_that_subscribes; crates/database/db/tests/merge/broadcast_mutator_broadcast.rs a_write_that_finds_no_subscriber_is_not_published_again",
    },
];
