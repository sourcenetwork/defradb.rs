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
