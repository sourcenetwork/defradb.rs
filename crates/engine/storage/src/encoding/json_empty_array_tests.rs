use document::{JsonLeafValue, JsonPath, JsonScalarValue};

use super::json::{
    decode_json_ascending, decode_json_descending, encode_json_ascending, encode_json_descending,
    JSON_EMPTY_ARRAY, JSON_EMPTY_ARRAY_DESC,
};

fn path() -> JsonPath {
    JsonPath::new().append_property("a").append_index()
}

fn values() -> Vec<JsonScalarValue> {
    vec![
        JsonScalarValue::Null,
        JsonScalarValue::Bool(false),
        JsonScalarValue::Bool(true),
        JsonScalarValue::Number(f64::MIN),
        JsonScalarValue::Number(-1.5),
        JsonScalarValue::Number(0.0),
        JsonScalarValue::Number(f64::MAX),
        JsonScalarValue::Number(f64::INFINITY),
        JsonScalarValue::String(String::new()),
        JsonScalarValue::String("\u{ff}\u{fe}".into()),
    ]
}

fn marker() -> JsonLeafValue {
    JsonLeafValue::new(path(), JsonScalarValue::EmptyArray)
}

#[test]
fn the_marker_round_trips_both_ways() {
    let ascending = encode_json_ascending(vec![], &marker());
    let (rest, decoded) = decode_json_ascending(&ascending).unwrap();
    assert!(rest.is_empty());
    assert_eq!(decoded, marker());

    let descending = encode_json_descending(vec![], &marker());
    let (rest, decoded) = decode_json_descending(&descending).unwrap();
    assert!(rest.is_empty());
    assert_eq!(decoded, marker());
}

#[test]
fn the_marker_sorts_after_every_value_ascending_and_before_descending() {
    let ascending = encode_json_ascending(vec![], &marker());
    let descending = encode_json_descending(vec![], &marker());
    for value in values() {
        let leaf = JsonLeafValue::new(path(), value);
        assert!(encode_json_ascending(vec![], &leaf) < ascending, "{leaf:?}");
        assert!(
            encode_json_descending(vec![], &leaf) > descending,
            "{leaf:?}"
        );
    }
}

#[test]
fn the_marker_stays_inside_its_path() {
    let max = JsonLeafValue::new(path(), JsonScalarValue::PathMax);
    let min = JsonLeafValue::new(path(), JsonScalarValue::PathMin);
    let ascending = encode_json_ascending(vec![], &marker());
    assert!(encode_json_ascending(vec![], &min) < ascending);
    assert!(ascending < encode_json_ascending(vec![], &max));
    let descending = encode_json_descending(vec![], &marker());
    assert!(encode_json_descending(vec![], &max) < descending);
    assert!(descending < encode_json_descending(vec![], &min));
}

#[test]
fn every_value_still_decodes_as_itself() {
    for value in values() {
        let leaf = JsonLeafValue::new(path(), value);
        let (_, ascending) = decode_json_ascending(&encode_json_ascending(vec![], &leaf)).unwrap();
        let (_, descending) =
            decode_json_descending(&encode_json_descending(vec![], &leaf)).unwrap();
        assert_eq!(ascending, leaf);
        assert_eq!(descending, leaf);
    }
}

/// `proofs/lean/OrderEncoding/JsonEmptyArray.lean` proves its laws at these bytes.
#[test]
fn the_markers_match_the_lean_model() {
    assert_eq!(JSON_EMPTY_ARRAY, 254);
    assert_eq!(JSON_EMPTY_ARRAY_DESC, 1);
}

#[test]
fn nested_array_paths_round_trip_descending() {
    let nested = path().append_index();
    for leaf in [
        marker(),
        JsonLeafValue::new(
            JsonPath::new()
                .append_property("orders")
                .append_index()
                .append_property("status")
                .append_index(),
            JsonScalarValue::EmptyArray,
        ),
        JsonLeafValue::new(nested.clone(), JsonScalarValue::EmptyArray),
        JsonLeafValue::new(path(), JsonScalarValue::Number(1.0)),
        JsonLeafValue::new(path(), JsonScalarValue::Null),
        JsonLeafValue::new(nested, JsonScalarValue::Null),
    ] {
        let encoded = encode_json_descending(vec![], &leaf);
        let (rest, decoded) =
            decode_json_descending(&encoded).unwrap_or_else(|e| panic!("{leaf:?}: {e}"));
        assert!(rest.is_empty());
        assert_eq!(decoded, leaf);
    }
}

#[test]
fn names_that_look_like_an_index_round_trip() {
    for name in ["w", "wx", "é", "\u{88}", "ww"] {
        for path in [
            JsonPath::new().append_property(name),
            JsonPath::new().append_property(name).append_index(),
            JsonPath::new().append_index().append_property(name),
        ] {
            let leaf = JsonLeafValue::new(path, JsonScalarValue::Number(1.0));
            let (_, ascending) =
                decode_json_ascending(&encode_json_ascending(vec![], &leaf)).unwrap();
            assert_eq!(ascending, leaf);
            let (_, descending) =
                decode_json_descending(&encode_json_descending(vec![], &leaf)).unwrap();
            assert_eq!(descending, leaf);
        }
    }
}
