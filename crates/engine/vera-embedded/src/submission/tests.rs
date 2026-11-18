use std::time::Duration;

use vera_jsonrpc::LocalRpcError;

use super::*;
use crate::error::EmbeddedError;

fn node_error(code: i32) -> Failure {
    Failure::from_node(
        "vera_getReceiptProof",
        LocalRpcError::Call {
            code,
            message: "proof capacity is occupied".into(),
            data: Some(r#"{"retryable":true}"#.into()),
        },
    )
}

#[test]
fn a_node_error_keeps_its_code_and_is_retried_as_it_is_over_http() {
    for (code, retried) in [
        (-32002, true),
        (-32000, true),
        (-32602, false),
        (-32601, false),
    ] {
        let error = client_error(node_error(code));
        assert!(
            matches!(&error, ClientError::Rpc { code: kept, message }
                if *kept == i64::from(code) && message == "proof capacity is occupied"),
            "{error:?}"
        );
        assert_eq!(error.retryable(), retried, "{code}");
        assert_eq!(
            error.to_string(),
            format!("RPC error ({code}): proof capacity is occupied")
        );
    }
}

#[test]
fn a_stopped_node_is_told_apart_from_a_busy_one_and_is_not_asked_again() {
    let stopped = client_error(Failure::from_node(
        "vera_sendNativeTx",
        LocalRpcError::Stopped,
    ));
    assert!(matches!(
        &stopped,
        ClientError::Transport {
            retryable: false,
            ..
        }
    ));
    assert!(!stopped.retryable());
    assert_eq!(
        stopped.to_string(),
        "transport: vera_sendNativeTx: the embedded Vera node has stopped"
    );

    let busy = client_error(node_error(-32002));
    assert!(matches!(busy, ClientError::Rpc { code: -32002, .. }));
    assert!(busy.retryable());
}

#[test]
fn an_oversized_or_malformed_result_is_refused_in_the_http_transports_terms() {
    assert!(matches!(
        client_error(Failure::TooLarge),
        ClientError::InvalidResponse("response exceeds byte limit")
    ));
    let malformed = serde_json::from_str::<Value>("{").unwrap_err();
    assert!(matches!(
        client_error(Failure::Malformed(malformed)),
        ClientError::Json(_)
    ));
}

#[test]
fn a_payload_the_node_cannot_take_is_not_asked_again() {
    for (error, said) in [
        (
            LocalRpcError::Payload("key must be a string".into()),
            "transport: vera_sendNativeTx: invalid JSON-RPC payload: key must be a string",
        ),
        (
            LocalRpcError::Subscription("7".into()),
            "transport: vera_sendNativeTx: subscription failed: 7",
        ),
    ] {
        let error = client_error(Failure::from_node("vera_sendNativeTx", error));
        assert!(!error.retryable());
        assert_eq!(error.to_string(), said);
    }
    let params = client_error(Failure::Channel(EmbeddedError::Params {
        method: "vera_sendNativeTx".into(),
        found: "an object",
    }));
    assert!(!params.retryable());
}

#[test]
fn a_node_that_gave_no_answer_in_time_is_asked_again_as_after_an_http_timeout() {
    let elapsed = client_error(Failure::Elapsed {
        method: "vera_getReceiptProof".into(),
        deadline: Duration::from_secs(5),
    });
    assert!(matches!(
        &elapsed,
        ClientError::Transport {
            retryable: true,
            ..
        }
    ));
    assert!(elapsed.retryable());
    assert_eq!(
        elapsed.to_string(),
        "transport: vera_getReceiptProof: no answer from the embedded Vera node within 5s"
    );
}

#[test]
fn a_wait_cut_short_by_a_stopping_runtime_is_not_asked_again() {
    let stopping = client_error(Failure::Channel(EmbeddedError::Runtime {
        method: "vera_getReceiptProof".into(),
        reason: edge::EdgeError::Closed,
    }));
    assert!(!stopping.retryable());
    assert_eq!(
        stopping.to_string(),
        "transport: vera_getReceiptProof: the network runtime is shutting down"
    );
}
