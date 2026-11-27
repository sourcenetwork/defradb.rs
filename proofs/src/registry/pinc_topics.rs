use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "pinc topics",
        name: "INV_Converges / INV_OnlyOwnTopics for a hub's missed report",
        axis: Tla,
        anchor: "crates/database/p2p/src/pinc/topics.rs topic_events; crates/database/p2p/src/sync/coordinator/event_handler/topic_missed.rs handle_topic_missed; crates/database/p2p/src/sync/coordinator/event_handler/doc_sync.rs handle_doc_sync_reply; crates/database/p2p/src/sync/coordinator/event_handler/branchable_sync.rs handle_branchable_sync_reply",
        model_ref: "MC_MissedTopicResync_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
