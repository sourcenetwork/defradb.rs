use std::time::Duration;

use serde_json::json;
use vera_jsonrpc::LocalRpcError;

use super::*;
use crate::error::EmbeddedError;

const METHOD: &str = "vera_getCurrentPermissionProof";

fn header(height: u64) -> GossipHeader {
    header_in(height, 0)
}

fn header_in(height: u64, term: u64) -> GossipHeader {
    let hash = format!("{:064x}", height + (term << 32));
    serde_json::from_value(json!({
        "chain_id": 1,
        "height": height,
        "block_hash": format!("0x{hash}"),
        "parent_hash": format!("0x{:064x}", height.saturating_sub(1)),
        "timestamp": 123,
        "term": term,
        "state_root": format!("0x{hash}"),
        "module_state_root": format!("0x{hash}"),
        "tx_count": 0,
        "publisher_index": 0,
        "signature": [],
    }))
    .expect("header")
}

#[test]
fn queued_headers_at_or_below_the_fetched_head_are_not_emitted() {
    let head = header(7);
    let queued = futures::stream::iter([
        Ok(header(6)),
        Ok(header(7)),
        Ok(header(8)),
        Ok(header(7)),
        Ok(header(6)),
    ])
    .boxed();
    let mut headers = headers_after(head, queued);

    let first = futures::executor::block_on(headers.next())
        .expect("head")
        .expect("head");
    let second = futures::executor::block_on(headers.next())
        .expect("next header")
        .expect("next header");
    let ended = futures::executor::block_on(headers.next());
    assert_eq!((first.height, second.height), (7, 8));
    assert!(ended.is_none());
}

/// A takeover of the deployment's active device voids blocks above its
/// cut: the next term's headers may be at or below the fetched head, and the
/// light client reads them there to learn its head was voided.
#[test]
fn a_header_of_a_later_term_at_or_below_the_fetched_head_is_emitted() {
    let head = header(7);
    let queued = futures::stream::iter([
        Ok(header(6)),
        Ok(header_in(6, 1)),
        Ok(header_in(7, 1)),
        Ok(header(7)),
    ])
    .boxed();
    let headers: Vec<(u64, u64)> =
        futures::executor::block_on(headers_after(head, queued).collect::<Vec<_>>())
            .into_iter()
            .map(|header| header.expect("header"))
            .map(|header| (header.height, header.term))
            .collect();
    assert_eq!(headers, [(7, 0), (6, 1), (7, 1)]);
}

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
