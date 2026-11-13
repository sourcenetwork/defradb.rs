use serde_json::json;

use super::*;

#[test]
fn a_result_is_refused_only_when_it_is_longer_than_the_limit() {
    let result = r#"{"height":7}"#;
    assert_eq!(result.len(), 12);
    for limit in [12, 13, usize::MAX] {
        assert_eq!(decode(result, limit).unwrap(), json!({ "height": 7 }));
    }
    for limit in [0, 11] {
        assert!(matches!(decode(result, limit), Err(Failure::TooLarge)));
    }
    assert_eq!(decode("null", 4).unwrap(), Value::Null);
}

#[test]
fn a_result_the_network_transports_accept_is_accepted_here() {
    let result = r#""0xabc""#;
    // The body a network client reads: the same result inside its envelope.
    let body = format!(r#"{{"jsonrpc":"2.0","id":1,"result":{result}}}"#);
    for limit in 0..=body.len() + 1 {
        let over_the_network = body.len() <= limit;
        assert!(!over_the_network || within(result.len(), limit));
    }
}

#[test]
fn a_result_that_is_not_json_is_reported_as_malformed() {
    assert!(matches!(decode("{", 16), Err(Failure::Malformed(_))));
}

#[test]
fn parameters_are_sent_by_position() {
    assert_eq!(
        positional("vera_getLightBlock", json!([7, "0x01"])).unwrap(),
        vec![json!(7), json!("0x01")]
    );
    assert!(positional("vera_nodeStatus", json!([])).unwrap().is_empty());
    assert!(positional("vera_nodeStatus", Value::Null)
        .unwrap()
        .is_empty());

    for (params, found) in [
        (json!({ "height": 7 }), "an object"),
        (json!("7"), "a string"),
        (json!(7), "a number"),
        (json!(true), "a boolean"),
    ] {
        let refused = positional("vera_getLightBlock", params).unwrap_err();
        assert_eq!(
            refused.to_string(),
            format!("vera_getLightBlock: parameters must be a JSON array, got {found}")
        );
    }
}
