use base64::Engine;
use chrono::{DateTime, Utc};
use defra_core::doc_id::ParsedDocId;
use serde_json::{Map, Value};

pub fn expected_length(value: &Value) -> Option<usize> {
    value.as_u64().and_then(|n| usize::try_from(n).ok())
}

pub fn match_length(expected: &Value, actual: &Value) -> bool {
    let Some(count) = expected_length(expected) else {
        return false;
    };
    match actual {
        Value::Array(items) => items.len() == count,
        Value::Object(map) => map.len() == count,
        Value::String(text) => text.len() == count,
        _ => false,
    }
}

pub fn match_nonempty(actual: &Value) -> bool {
    match actual {
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        Value::String(text) => !text.is_empty(),
        _ => false,
    }
}

pub fn valid_doc_id(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|text| ParsedDocId::from_string(text).is_ok())
}

pub fn match_current_timestamp(value: &Value, now: DateTime<Utc>) -> bool {
    let Some(timestamp) = value
        .as_str()
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
    else {
        return false;
    };
    (now - timestamp.with_timezone(&Utc)).abs() <= chrono::Duration::seconds(120)
}

pub fn not_plain_cbor_string(actual: &Value, value: &str) -> bool {
    let Some(encoded) = actual.as_str() else {
        return false;
    };
    let Ok(data) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
        return false;
    };
    let length = value.len() as u64;
    let mut expected = match length {
        0..=23 => vec![0x60 | length as u8],
        24..=0xff => vec![0x78, length as u8],
        0x100..=0xffff => [&[0x79u8][..], &(length as u16).to_be_bytes()[..]].concat(),
        0x1_0000..=0xffff_ffff => [&[0x7au8][..], &(length as u32).to_be_bytes()[..]].concat(),
        _ => [&[0x7bu8][..], &length.to_be_bytes()[..]].concat(),
    };
    expected.extend_from_slice(value.as_bytes());
    data != expected
}

fn finite(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite())
}

pub fn valid_similarity(map: &Map<String, Value>) -> bool {
    let metric_ok = matches!(
        map.get("metric").and_then(Value::as_str),
        Some("COSINE" | "EUCLIDEAN" | "DOT")
    );
    let (Some(source), Some(vector)) = (
        map.get("source").and_then(Value::as_array),
        map.get("vector").and_then(Value::as_array),
    ) else {
        return false;
    };
    map.len() == 4
        && metric_ok
        && source.len() == vector.len()
        && source.iter().chain(vector).all(|v| finite(v).is_some())
}

pub fn match_similarity(want: &Map<String, Value>, actual: &Value) -> bool {
    let Some(got) = finite(actual) else {
        return false;
    };
    let numbers = |key: &str| -> Vec<f64> {
        want[key]
            .as_array()
            .map(|items| items.iter().filter_map(finite).collect())
            .unwrap_or_default()
    };
    let (source, vector) = (numbers("source"), numbers("vector"));
    let (mut dot, mut squares, mut source_norm, mut vector_norm) = (0.0, 0.0, 0.0, 0.0);
    for (s, v) in source.iter().zip(&vector) {
        dot += s * v;
        squares += (s - v) * (s - v);
        source_norm += s * s;
        vector_norm += v * v;
    }
    let expected = match want["metric"].as_str() {
        Some("DOT") => dot,
        Some("EUCLIDEAN") => -squares,
        _ if source_norm != 0.0 && vector_norm != 0.0 => {
            dot / (f64::sqrt(source_norm) * f64::sqrt(vector_norm))
        }
        _ => 0.0,
    };
    (got - expected).abs() <= 1e-6
}
