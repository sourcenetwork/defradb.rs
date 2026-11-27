//! The reply correlators: a reply reaches the request registered for its
//! message ID once, and a request that is gone or replaced gets none.

use p2p::message::{ManageQueryReply, ManageReply, QuerySEArtifactsReply};
use p2p::{Error, ManageCorrelator, ManageQueryCorrelator, SeQueryCorrelator};

fn se_reply(message_id: &str) -> QuerySEArtifactsReply {
    QuerySEArtifactsReply {
        message_id: message_id.to_string(),
        doc_ids: vec!["doc-1".to_string()],
        ..Default::default()
    }
}

#[nuclei::test(threads = 0)]
async fn a_reply_reaches_its_waiting_request_once() {
    let correlator = SeQueryCorrelator::new();
    let mut pending = correlator.register("query-1".to_string());

    assert!(correlator.deliver(se_reply("query-1")));
    assert!(
        !correlator.deliver(se_reply("query-1")),
        "the slot is spent"
    );
    assert_eq!(pending.recv().await.unwrap().doc_ids, ["doc-1"]);
    assert_eq!(correlator.in_flight(), 0);
}

#[nuclei::test(threads = 0)]
async fn a_dropped_request_takes_its_slot_with_it() {
    let correlator = SeQueryCorrelator::new();
    drop(correlator.register("query-1".to_string()));

    assert_eq!(correlator.in_flight(), 0);
    assert!(!correlator.deliver(se_reply("query-1")));
}

#[nuclei::test(threads = 0)]
async fn a_replaced_or_cancelled_request_ends_without_a_reply() {
    let correlator = SeQueryCorrelator::new();
    let mut first = correlator.register("query-1".to_string());
    let mut second = correlator.register("query-1".to_string());

    assert!(matches!(first.recv().await, Err(Error::ChannelReceive)));
    correlator.cancel("query-1");
    assert!(matches!(second.recv().await, Err(Error::ChannelReceive)));
}

#[nuclei::test(threads = 0)]
async fn manage_replies_route_by_message_id() {
    let correlator = ManageCorrelator::new();
    let mut pending = correlator.register("manage-1".to_string());
    let stray = ManageReply {
        message_id: "other".to_string(),
        ..Default::default()
    };
    assert!(!correlator.deliver(stray));
    assert!(correlator.deliver(ManageReply {
        message_id: "manage-1".to_string(),
        ..Default::default()
    }));
    assert_eq!(pending.recv().await.unwrap().message_id, "manage-1");

    let queries = ManageQueryCorrelator::new();
    let mut query = queries.register("query-1".to_string());
    queries.cancel("query-1");
    assert!(matches!(query.recv().await, Err(Error::ChannelReceive)));
    assert!(!queries.deliver(ManageQueryReply {
        message_id: "query-1".to_string(),
        ..Default::default()
    }));
}
