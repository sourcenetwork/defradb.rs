use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Pubsub RPC reply addressing",
        name: "INV_NoAnswerLost / INV_AllAnswered / LIVE_Done / LIVE_AllArrive: a peer answers a request on its publisher's reply topic, even when a relay handed it the request, so no answer is lost and the asker's wait ends with every answer, or at its deadline while some answer is still missing or on its way",
        axis: Tla,
        anchor: "crates/database/p2p/src/host/p2p_host/protocols.rs handle_gossipsub_event; crates/database/p2p/src/sync/coordinator/event_handler/pubsub_raw.rs handle_gossip_raw_message; crates/database/p2p/src/pubsub_rpc/topic_handler.rs TopicHandler::deliver_gossip_message; crates/database/p2p/src/sync/coordinator/pubsub_client.rs pubsub_sync_branchable_collection; crates/database/p2p/src/iroh/gossip/delivery.rs raw_publisher",
        model_ref: "MC_PubsubRpcReply_{Green,Red_AnswerRelay,Red_DeadlineCutsOff}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/event_handler/mod.rs relayed_pubsub_rpc_request_is_answered_on_its_publishers_topic, relayed_kms_request_reaches_the_handler_as_its_publisher; crates/database/p2p/tests/pubsub_rpc_relayed_request.rs a_relayed_request_names_its_publisher; crates/database/p2p/tests/iroh/relayed_raw_request.rs a_relayed_raw_request_names_its_publisher; crates/database/p2p/src/iroh/gossip/delivery.rs a_sealed_raw_message_names_its_publisher_not_the_relay",
    },
    Property {
        family: "Pubsub RPC reply addressing",
        name: "open_seal / open_seal_never_unsigned / open_unsigned / open_refuses_short: an iroh raw payload sealed by its publisher opens to that publisher, is never taken for an unsealed one, and an unsealed one passes through untouched",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/gossip/raw_origin.rs seal, open; crates/database/p2p/src/iroh/gossip/delivery.rs raw_publisher",
        model_ref: "GossipFrame (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/tests/iroh/gossip_raw_origin.rs sealed_payload_opens_to_its_origin, a_truncated_seal_is_refused_not_passed_through, an_unsealed_cbor_payload_passes_through",
    },
];
