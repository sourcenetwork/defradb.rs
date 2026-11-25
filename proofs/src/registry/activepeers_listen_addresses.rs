use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "ActivePeers listen addresses",
        name: "INV_ListenAddress / INV_NotHeldByLater / LIVE_Answered: ActivePeers never lists a peer that was waiting at the call at its source port while its Identify may still come, a peer that dials in after the call never holds it, and every call is answered",
        axis: Tla,
        anchor: "crates/database/p2p/src/host/command_handler/network.rs handle_peer_addresses, on_identify_settled, answer_peer_addresses; crates/database/p2p/src/host/p2p_host/swarm.rs handle_swarm_event, handle_connection_closed; crates/database/p2p/src/host/p2p_host/protocols.rs handle_identify_event",
        model_ref: "MC_ActivePeersIdentify_{Green,Red_AnswerAtOnce,Red_WaitOnLater}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/host/active_peers_tests.rs active_peers_lists_a_dialer_at_its_listen_address_before_its_identify, active_peers_is_not_held_by_a_peer_that_dials_in_after_the_call",
    },
];
