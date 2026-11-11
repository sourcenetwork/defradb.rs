use defra_core::{decode_lens_argument, encode_lens_argument};
use serde_json::{json, Value};

fn encoded(json_text: &str) -> String {
    encode_lens_argument(&serde_json::from_str::<Value>(json_text).unwrap())
}

#[test]
fn numbers_keep_the_value_go_decodes_with_use_number() {
    assert_eq!(encoded("9007199254740993"), "9007199254740993");
    assert_eq!(encoded("9007199254740992"), "9007199254740992");
    assert_eq!(encoded("-42"), "-42");
    assert_eq!(encoded("30.0"), "30.0");
}

/// Go writes an exponent as it arrived; serde_json respells it. The spelling
/// is part of the lens ID, so a formatter change must show up here.
#[test]
fn an_exponent_takes_serde_jsons_spelling() {
    assert_eq!(encoded("1e21"), "1e+21");
    assert_eq!(encoded("1e+21"), "1e+21");
}

#[test]
fn nested_object_keys_are_sorted_as_go_marshals_a_map() {
    assert_eq!(
        encoded(r#"{"b":1,"a":{"d":[{"z":0,"y":1}],"c":3}}"#),
        r#"{"a":{"c":3,"d":[{"y":1,"z":0}]},"b":1}"#
    );
}

#[test]
fn strings_take_gos_html_escapes() {
    assert_eq!(
        encode_lens_argument(&json!("<a&b>\u{2028}")),
        "\"\\u003ca\\u0026b\\u003e\\u2028\""
    );
}

#[test]
fn a_string_that_looks_like_json_round_trips_as_a_string() {
    for value in [json!("42"), json!("true"), json!("null"), json!("[1]")] {
        assert_eq!(decode_lens_argument(&encode_lens_argument(&value)), value);
    }
}

#[test]
fn a_value_that_is_not_json_decodes_as_its_raw_string() {
    assert_eq!(decode_lens_argument("verified"), json!("verified"));
}
