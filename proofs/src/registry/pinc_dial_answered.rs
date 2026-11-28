use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // pinc hands a dial over before the peer answers when its access was
        // allowed lately; the endpoint counts the peer only once it answered.
        family: "pinc dial answered",
        name: "INV_NoPushUnanswered / INV_ConnectedHeard: a dial counts the peer as connected only once it answered, so the retry pass never pushes into a peer that stopped",
        axis: Tla,
        anchor: "crates/database/p2p/src/pinc/endpoint.rs Node::connection, Node::connected, Node::disconnected; crates/database/p2p/src/pinc/command.rs Command::ConnectedPeers; crates/surfaces/p2p-adapter/src/retry.rs run_retry_pass",
        model_ref: "MC_PincDialAnswered_{Green,Red_CountAtHandOver}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/pinc/tests/redial.rs a_dial_to_a_peer_that_stopped_fails_and_it_stays_disconnected",
    },
];
