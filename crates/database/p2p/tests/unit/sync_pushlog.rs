use std::time::Duration;

use crypto::generate_ed25519;
use identity::{Identity, RawIdentity};

use super::*;

#[test]
fn in_flight_single_flight_suppression_replies_with_backpressure() {
    let result = Err(crate::error::Error::PushLogInFlight {
        cid: "bafy-head".to_string(),
    });

    let reply = build_pushlog_reply("message-1", &result, true);
    assert_eq!(reply.retry_after_ms, None);

    assert_eq!(
        reply.err_message.as_deref(),
        Some(crate::error::RATE_LIMITED_MESSAGE)
    );
}

#[test]
fn pending_durability_does_not_pause_the_peer() {
    let result = Err(crate::error::Error::PushLogNotDurable { cid: "head".into() });
    let reply = build_pushlog_reply("message", &result, true);
    assert_eq!(reply.retry_after_ms, None);
    assert_eq!(
        reply.err_message.as_deref(),
        Some(crate::error::RATE_LIMITED_MESSAGE)
    );
}

#[test]
fn peer_capacity_hint_requires_negotiation() {
    let result = Err(crate::error::Error::PendingDagCapacity { max: 1 });
    for negotiated in [false, true] {
        let reply = build_pushlog_reply("message", &result, negotiated);
        assert_eq!(reply.retry_after_ms, negotiated.then_some(2000));
    }
}

#[test]
fn verifies_capability_embedded_by_transport_generic_sender() {
    let authorizer = RawIdentity::from_private_key(generate_ed25519().unwrap()).unwrap();
    let mut request = crate::message::PushLogRequest::new(
        "doc".to_string(),
        Vec::new().into(),
        "collection".to_string(),
        authorizer.did().unwrap().to_string(),
        Vec::new().into(),
    );
    request.explicit_replay_capability = Some(
        crate::generate_explicit_replay_capability(
            &authorizer,
            "source",
            "target",
            "collection",
            Duration::from_secs(60),
        )
        .unwrap(),
    );

    let authorization = verify_embedded_replay_capability(&request, "source", "target").unwrap();

    assert_eq!(authorization.authorizer_did, request.creator);
}
