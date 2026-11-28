use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Inbound stream admission",
        name: "INV_ResidentWithinBudget / INV_ShareBound / LIVE_HealthyHandled / LIVE_WaitEnds",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_streams.rs admit_stream; crates/database/p2p/src/iroh/inbound_budget.rs InboundBudget::admit, InboundBudget::peer_share, InboundDeadlines::body; crates/database/p2p/src/sync/dispatch_queue.rs dispatch_queue; crates/database/p2p/src/sync/event_dispatcher.rs run_event_dispatcher",
        model_ref: "MC_InboundAdmission_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A request shed while every rejection worker is busy is dropped. Its
        // stream closes empty, so the requester sees Refused (CAR) or
        // Transport (PushLog) instead of Busy, and spends free refusals, stall
        // budget and an attempt. Retries stay bounded by DagFetchBudget and the
        // per-root clock.
        family: "Inbound stream admission",
        name: "A request shed past the rejection workers is answered by a closed stream, not Busy",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/event_dispatcher.rs run_event_dispatcher, MAX_ACTIVE_REJECTIONS; crates/database/p2p/src/iroh/endpoint_rpc.rs CarFetchOutcome",
        model_ref: "MC_InboundAdmission_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A CAR serve that finds its lane full waits a bounded time for a
        // slot; the freed slot goes to the peer running the fewest serves.
        // Past the wait, or with the room full, it is answered busy (#2536).
        family: "Inbound stream admission",
        name: "INV_Capacity / INV_SlotsAccounted / INV_WorkConserving / INV_RoomBounded / INV_BusyOnlyWhenFull / INV_FairGrant / LIVE_Answered: a full CAR-serve lane waits, grants fairly and ends every request served, busy or closed",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/car_serve_admission.rs ServeAdmission::offer, ServeAdmission::release, ServeAdmission::pop_expired; crates/database/p2p/src/sync/event_dispatcher.rs run_event_dispatcher, spawn_serve, refuse, ServeSlot",
        model_ref: "MC_CarServeAdmission_{Green,Red_NoCapacity,Red_ShedAtOnce,Red_UnboundedRoom,Red_NoGrant,Red_FifoGrant,Red_LeakOnFail}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/event_dispatcher_serve_tests.rs a_serve_that_finds_the_lane_full_waits_for_a_slot, a_wait_that_runs_out_is_answered_busy, a_freed_slot_goes_to_the_peer_holding_the_fewest_serves, a_peer_past_its_share_of_the_waiting_room_is_answered_busy_at_once",
    },
    Property {
        family: "Inbound stream admission",
        name: "empty_healthy / offer_healthy / offer_waits_only_when_full / offer_admits_when_free / offer_sheds_only_when_room_full / release_healthy / release_grants_a_waiter / expire_healthy",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/car_serve_admission.rs ServeAdmission::offer, ServeAdmission::release, ServeAdmission::pop_expired",
        model_ref: "CarServeAdmission (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/car_serve_admission_tests.rs a_full_lane_holds_the_request_until_a_slot_frees, the_room_is_bounded_overall_and_per_peer, an_expired_wait_is_returned_to_be_answered_busy",
    },
    Property {
        family: "Inbound stream admission",
        name: "INV_OnlyRegisteredServed / INV_ShareBound / INV_RoomIsTheRequest / INV_RevokedIsCut / INV_ShutdownLeavesNone / LIVE_HandlerEnds",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/endpoint_streams.rs admit_stream, handle_connection_streams, serve_protocol_stream; crates/database/p2p/src/iroh/stream_protocols.rs StreamProtocols::get, ProtocolRequest::release, StreamHandler::handle; crates/database/p2p/src/iroh/endpoint_commands.rs handle_deny_peer; crates/database/p2p/src/iroh/task_registry.rs TaskRegistry::close",
        model_ref: "MC_EmbedderStream_{Green,Red_AnyTag,Red_UntilReturn,Red_NoDeadline,Red_BarOnly,Red_NoAbort}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Inbound stream admission",
        name: "register_all_valid / register_serves_each_as_given / reserved_tags_find_nothing / unregistered_tags_find_nothing / a_repeated_tag_is_refused",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/stream_protocols.rs StreamProtocol::validate, StreamProtocols::new, StreamProtocols::get",
        model_ref: "StreamProtocols (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Inbound stream admission",
        name: "acquire_conserves / release_all_restores / admit_failure_keeps_nothing",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/inbound_budget.rs InboundBudget::admit, InboundLease",
        model_ref: "ByteBudget (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
