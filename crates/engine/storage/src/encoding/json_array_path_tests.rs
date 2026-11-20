use document::{JsonLeafValue, JsonPath, JsonScalarValue};

use super::{decode_json_descending, encode_json_descending};

#[test]
fn descending_json_array_path_round_trips() {
    let leaf = JsonLeafValue::new(
        JsonPath::new().append_property("custom").append_index(),
        JsonScalarValue::Number(2.0),
    );

    let encoded = encode_json_descending(Vec::new(), &leaf);
    let (remaining, decoded) = decode_json_descending(&encoded).unwrap();

    assert!(remaining.is_empty());
    assert_eq!(decoded.path, leaf.path);
    assert_eq!(decoded.value, leaf.value);
}

#[test]
fn descending_json_array_path_avoids_property_collision() {
    let array_path = JsonLeafValue::new(
        JsonPath::new().append_property("custom").append_index(),
        JsonScalarValue::Number(2.0),
    );
    let property_path = JsonLeafValue::new(
        JsonPath::new().append_property("customw"),
        JsonScalarValue::Number(2.0),
    );

    let encoded_array = encode_json_descending(Vec::new(), &array_path);
    let encoded_property = encode_json_descending(Vec::new(), &property_path);

    assert_ne!(encoded_array, encoded_property);
}
