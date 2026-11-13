use std::time::Duration;

use vera_jsonrpc::LocalRpcError;

use super::*;
use crate::error::EmbeddedError;

const METHOD: &str = "vera_getCurrentPermissionProof";

#[test]
fn a_node_error_reads_as_it_does_over_http() {
    let error = transport_error(
        METHOD,
        Failure::from_node(
            METHOD,
            LocalRpcError::Call {
                code: -32002,
                message: "proof capacity is occupied".into(),
                data: None,
            },
        ),
    );
    assert!(matches!(
        &error,
        TransportError::Rpc { method: METHOD, code: -32002, message }
            if message == "proof capacity is occupied"
    ));
    assert_eq!(
        error.to_string(),
        "vera_getCurrentPermissionProof RPC error (-32002): proof capacity is occupied"
    );
}

#[test]
fn an_oversized_result_is_refused_as_it_is_over_http() {
    let error = transport_error(METHOD, Failure::TooLarge);
    assert!(matches!(
        error,
        TransportError::ResponseTooLarge { method: METHOD }
    ));
    assert_eq!(
        error.to_string(),
        "vera_getCurrentPermissionProof response exceeds byte limit"
    );
}

#[test]
fn a_stopped_node_is_a_channel_failure_the_caller_can_recognise() {
    let error = transport_error(METHOD, Failure::from_node(METHOD, LocalRpcError::Stopped));
    let TransportError::Other(source) = &error else {
        panic!("expected a channel failure, got {error:?}");
    };
    assert!(matches!(
        source.downcast_ref::<EmbeddedError>(),
        Some(EmbeddedError::Stopped { method }) if method == METHOD
    ));
    assert_eq!(
        error.to_string(),
        "vera_getCurrentPermissionProof: the embedded Vera node has stopped"
    );
}

#[test]
fn a_refused_subscription_names_the_subscription() {
    let error = header_error(Failure::from_node(
        SUBSCRIBE_HEADERS,
        LocalRpcError::Subscription("too many subscriptions".into()),
    ));
    assert_eq!(
        error.to_string(),
        "vera_subscribeHeaders: subscription failed: too many subscriptions"
    );
}

#[test]
fn a_malformed_result_keeps_the_decoder_s_words() {
    let malformed = serde_json::from_str::<Value>("{").unwrap_err();
    let words = malformed.to_string();
    assert_eq!(
        transport_error(METHOD, Failure::Malformed(malformed)).to_string(),
        words
    );
}

#[test]
fn a_read_or_a_subscription_left_unanswered_names_its_method_and_deadline() {
    // The deadlines of Backbone's HTTP and WebSocket transport.
    assert_eq!(deadline::PROOF_READ, Duration::from_secs(10));
    assert_eq!(deadline::SUBSCRIPTION, Duration::from_secs(10));

    let read = transport_error(
        METHOD,
        Failure::Elapsed {
            method: METHOD.into(),
            deadline: deadline::PROOF_READ,
        },
    );
    let TransportError::Other(source) = &read else {
        panic!("expected a channel failure, got {read:?}");
    };
    assert!(matches!(
        source.downcast_ref::<EmbeddedError>(),
        Some(EmbeddedError::Elapsed { method, deadline })
            if method == METHOD && *deadline == Duration::from_secs(10)
    ));
    assert_eq!(
        read.to_string(),
        "vera_getCurrentPermissionProof: no answer from the embedded Vera node within 10s"
    );

    let subscription = header_error(Failure::Elapsed {
        method: SUBSCRIBE_HEADERS.into(),
        deadline: deadline::SUBSCRIPTION,
    });
    assert_eq!(
        subscription.to_string(),
        "vera_subscribeHeaders: no answer from the embedded Vera node within 10s"
    );
}
