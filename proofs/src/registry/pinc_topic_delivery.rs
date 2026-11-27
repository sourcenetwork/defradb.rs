use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A pinc hub pushes a topic's messages to every member; the endpoint
        // hands one to the engine only while it holds a peer session, marks a
        // raw topic before joining it, and reports JOINED from pinc's
        // neighbours, the members it holds a session with. Every publish
        // holds its topic joined for gossipsub's fanout time, so the engine
        // hears who joins a topic it only publishes to; an unsubscribe leaves
        // at once and ends the hold.
        family: "pinc topic delivery",
        name: "INV_NoDeliveryOffMesh / INV_RawNeverDecoded / INV_JoinedOnlyLinkedMembers / INV_EngineKeepsItsTopic / INV_OnlyEngineTopics / INV_HeldIsJoined: only a node on the mesh gets a message, a raw topic is never decoded, JOINED names a linked member, an ended hold never takes the engine off its topic, the engine gets messages only of topics it is subscribed to and JOINED only of those or held ones, and a held topic stays joined",
        axis: Tla,
        anchor: "crates/database/p2p/src/pinc/topics.rs Node::subscribe, Node::unsubscribe, Node::leave, Node::publish, Node::hold_fanout, Node::expire_fanout, Node::topic_events, Node::delivery; crates/database/p2p/src/pinc/endpoint.rs Node::connected, Node::disconnected, Node::sweep_loop",
        model_ref: "MC_PincTopicDelivery_{Green,Green_Raw,Red_NoGate,Red_RawLate,Red_NoSessionFilter,Red_NoOwnerCheck,Red_NoHoldEnd,Red_NoEngineFilter}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/p2p/src/pinc/tests/topics.rs connected_subscribers_see_each_other_join_and_leave, a_publisher_that_never_subscribed_sees_subscribers_join_and_leave, a_node_with_no_peer_takes_no_head_hint, raw_topics_carry_a_request_and_its_answer_as_bytes",
    },
];
