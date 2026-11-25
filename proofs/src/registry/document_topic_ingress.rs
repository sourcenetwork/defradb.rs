use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p, p2p-adapter (#2545)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // An update on a document's topic is applied only while the node
        // follows the document or its collection: an unsubscribe clears the
        // set the gossip handler reads before it returns, so updates queued or
        // sent after it are dropped.
        family: "Document topic ingress",
        name: "INV_NoApplyAfterUnsubscribe / INV_FollowedApplies: no update is applied after the document's unsubscribe returned unless its collection is followed, and none taken while followed is dropped",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/event_handler/gossip.rs handle_gossip_message; crates/database/p2p/src/sync/coordinator/access.rs is_followed_document; crates/surfaces/p2p-adapter/src/libp2p.rs P2PAdapter::add_documents, remove_documents; crates/surfaces/p2p-adapter/src/iroh.rs IrohP2PAdapter::add_documents, remove_documents; crates/entry-points/defra-node/src/libp2p_recovery.rs restore_libp2p_documents; crates/surfaces/p2p-adapter/src/iroh_restore.rs restore_iroh_p2p_state",
        model_ref: "MC_DocTopicIngress_{Green,Red_NoCheck,Red_OwnSet,Red_NoCollection}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/sync/coordinator/access_tests.rs gossip_document_topic_dropped_after_document_unfollowed; crates/surfaces/p2p-adapter/tests/libp2p/followed_documents.rs remove_documents_clears_the_shared_followed_set",
    },
];
