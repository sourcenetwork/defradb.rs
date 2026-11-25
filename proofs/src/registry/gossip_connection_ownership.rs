use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Gossip connection ownership",
        name: "INV_RevokedHasNoGossip / INV_PathOnLiveConnection / INV_GossipNeverClosesShared / LIVE_HealthyReached / LIVE_AskAnswered",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/gossip/driver.rs Driver::send, Driver::connected, Driver::forget; crates/database/p2p/src/iroh/gossip/mod.rs GossipHandle::connected; crates/database/p2p/src/iroh/endpoint_rpc.rs connect_with_cache; crates/database/p2p/src/iroh/endpoint_commands.rs handle_deny_peer",
        model_ref: "MC_GossipOwnership_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Gossip connection ownership",
        name: "LIVE_RequestAnswered",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/gossip/inbox.rs Inbox::push, Inbox::offer, Inbox::next; crates/database/p2p/src/iroh/gossip/driver.rs read_stream, Driver::emit; crates/database/p2p/src/iroh/records/task.rs RecordsHandle::push_change, RecordsHandle::push, RecordsHandle::next",
        model_ref: "MC_GossipInbox_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Gossip connection ownership",
        name: "decode_encode / decode_all_encode_all / decode_rejects_oversized / read_tag_write_tag",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/gossip/frame.rs write_gossip_frame, read_gossip_frame; crates/database/p2p/src/iroh/protocols.rs write_stream_tag, read_stream_tag",
        model_ref: "GossipFrame (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Gossip connection ownership",
        name: "broadcast_fits_frame / emitted_fits_frame / max_broadcast_fits / over_max_broadcast_dropped / sealed_raw_fits",
        axis: Lean,
        anchor: "crates/database/p2p/src/iroh/gossip/frame.rs MAX_MESSAGE_SIZE, TOPIC_ID_LEN, FRAME_LIMIT, BROADCAST_OVERHEAD, MAX_BROADCAST, max_broadcast; crates/database/p2p/src/iroh/gossip/driver.rs config; crates/database/p2p/src/iroh/gossip/raw_origin.rs HEADER_LEN, MAX_RAW_PAYLOAD; crates/database/p2p/src/iroh/transport.rs publish_raw",
        model_ref: "GossipFrame.Budget (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/tests/iroh/direct_push.rs a_raw_publish_too_large_for_gossip_is_refused",
    },
    Property {
        family: "Gossip connection ownership",
        name: "INV_NoSilentLoss / INV_BroadcastFits",
        axis: Tla,
        anchor: "crates/database/p2p/src/iroh/gossip/mod.rs GossipHandle::publish; crates/database/p2p/src/iroh/gossip/driver.rs write_stream, write_queue; crates/database/p2p/src/iroh/endpoint_commands.rs handle_command; crates/database/p2p/src/iroh/direct_push.rs peers, push, push_one, FANOUT",
        model_ref: "MC_GossipDelivery_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
