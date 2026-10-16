mod common;

use common::fixtures::{assert_same_frame, canonical, decode, encode, load, replay, s, sha256_hex};
use serde_json::Value;

fn codec_case<'a>(cases: &'a [Value], name: &str) -> &'a Value {
    cases
        .iter()
        .find(|c| s(c, "name") == name)
        .unwrap_or_else(|| panic!("no codec case {name}"))
}

#[test]
fn codec_fixtures() {
    for case in load("codec.json") {
        let name = s(&case, "name");
        let protocol = s(&case, "protocol");
        let ours = encode(protocol, replay(case["build"].as_array().unwrap()));

        if case["exact"].as_bool().unwrap() {
            assert_same_frame(&case, &ours, name);
        }
        assert_decodes_like_reference(&case, &ours, "our frame");
        if let Some(golden) = case["frame_hex"].as_str() {
            assert_decodes_like_reference(&case, &hex::decode(golden).unwrap(), "golden frame");
        }
    }
}

/// v0 frames carry no cid, so the reference decode (a CIDv0 block) differs from the built
/// message; compare against what the reference implementation decoded from its own frame.
fn assert_decodes_like_reference(case: &Value, frame: &[u8], what: &str) {
    let name = s(case, "name");
    let decoded = decode(s(case, "protocol"), frame);
    if case["decode_error"].is_string() {
        assert!(decoded.is_err(), "{name}: {what} should fail to decode");
    } else {
        let m = decoded.unwrap_or_else(|e| panic!("{name}: {what} failed to decode: {e}"));
        assert_eq!(
            canonical(&m),
            case["decoded_message"],
            "{name}: decode of {what}"
        );
    }
}

/// A large round-trip input is stored as a digest plus the codec case that produced it; the
/// frame is rebuilt with our encoder and accepted only if it hashes to the golden digest.
fn decode_input(case: &Value, codec_cases: &[Value]) -> Vec<u8> {
    if let Some(frame) = case["frame_hex"].as_str() {
        return hex::decode(frame).unwrap();
    }
    let source = codec_case(codec_cases, s(case, "build_from"));
    let frame = encode(
        s(source, "protocol"),
        replay(source["build"].as_array().unwrap()),
    );
    assert_eq!(
        sha256_hex(&frame),
        s(case, "frame_sha256"),
        "{}: rebuilt input differs from the golden frame",
        s(case, "name")
    );
    frame
}

#[test]
fn decode_fixtures() {
    let codec_cases = load("codec.json");
    for case in load("decode.json") {
        let name = s(&case, "name");
        let frame = decode_input(&case, &codec_cases);
        let result = decode(s(&case, "protocol"), &frame);
        match s(&case, "result") {
            "ok" => {
                let mut m = result.unwrap_or_else(|e| panic!("{name}: expected ok, got {e}"));
                assert_eq!(canonical(&m), case["message"], "{name}: message");
                m.verify_blocks();
                assert_eq!(canonical(&m), case["verified_message"], "{name}: verified");
            }
            "err" => assert!(result.is_err(), "{name}: expected an error"),
            o => panic!("{name}: result {o}"),
        }
    }
}
