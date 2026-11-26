use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Connection manager trims",
        name: "INV_NoRedialOfPruned / INV_TrimsSpaced / INV_ExplicitNeverRefused / INV_InFlightDialNotDenied: neither end's Kademlia redials a peer inside the silence period after its last connection closed, by a trim or otherwise; two trims are at least a silence period apart, as go-libp2p's connmgr; the hold never refuses an explicit dial; and a Kademlia dial that meets a dial in flight is never refused as held",
        axis: Tla,
        anchor: "crates/database/p2p/src/host/p2p_host/connection_manager.rs prune_candidates, silenced; crates/database/p2p/src/behaviour/dual_kademlia.rs poll, handle_pending_outbound_connection, on_swarm_event, refuse_held_dial; crates/database/p2p/src/behaviour/redial_hold.rs hold, is_held",
        model_ref: "MC_ConnmgrTrimSilence_{Green,Red_NoSilence,Red_NoHold,Red_HoldAll,Red_RefuseFirst,Red_TrimOnly}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/host/p2p_host/connection_manager.rs no_trim_runs_during_the_silence_period, a_trim_that_closes_nothing_does_not_silence_the_next; crates/database/p2p/src/behaviour/redial_hold.rs a_hold_lasts_until_its_deadline, a_later_disconnect_restarts_the_hold; crates/database/p2p/src/behaviour/dual_kademlia_tests.rs its_own_dial_to_a_held_peer_is_refused_once_it_reaches_the_swarm, another_behaviours_dial_to_a_held_peer_is_permitted, a_dial_that_failed_its_peer_condition_is_forgotten, every_dial_kademlia_asks_for_is_remembered_as_its_own, kademlias_dial_after_the_last_connection_closes_is_refused, closing_one_of_several_connections_holds_nothing",
    },
    Property {
        family: "Connection manager trims",
        name: "RedialHold.hold_bounded / hold_latest_in_taken / hold_holds / live_hold_stays_held / forget_replaced / forget_by_peer_drops_live: the redial hold table never grows past its cap, names only holds its queue can still forget, holds a peer exactly until its deadline, never drops a live hold when forgetting expired or replaced entries, and a forget by peer alone would",
        axis: Lean,
        anchor: "crates/database/p2p/src/behaviour/redial_hold.rs RedialHold::hold, is_held, forget_expired, forget, MAX_HELD",
        model_ref: "RedialHold (lake build)",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/src/behaviour/redial_hold.rs a_hold_lasts_until_its_deadline, a_later_disconnect_restarts_the_hold, expired_holds_are_forgotten, a_full_table_evicts_its_oldest_hold, evicting_a_replaced_hold_keeps_the_peer_held",
    },
];
