//! DateTime inputs and outputs match Go (#2364): a `[DateTime]` element may
//! be `UTC_NOW`, a malformed element's error names it, and a mutation renders
//! a time as Go's `time.RFC3339Nano` does.

use chrono::DateTime;
use document::NormalValue;
use query::plan::mutation::{json_to_normal_value_with_kind, normal_value_to_json};
use schema::{FieldKind, ScalarArrayKind};
use serde_json::json;

const ARRAY_KINDS: [ScalarArrayKind; 2] = [
    ScalarArrayKind::DateTimeArray,
    ScalarArrayKind::NillableDateTimeArray,
];

#[test]
fn a_malformed_date_time_array_element_is_named_in_the_error() {
    for kind in ARRAY_KINDS {
        let error = json_to_normal_value_with_kind(
            &json!(["2024-01-01T00:00:00Z", "not-a-date"]),
            Some(&FieldKind::ScalarArray(kind)),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("not-a-date"),
            "{kind:?}: {error}"
        );
    }
}

#[test]
fn utc_now_is_accepted_in_a_date_time_array() {
    let before = chrono::Utc::now();
    for kind in ARRAY_KINDS {
        let value = json_to_normal_value_with_kind(
            &json!(["UTC_NOW"]),
            Some(&FieldKind::ScalarArray(kind)),
        )
        .unwrap();
        let time = match value {
            NormalValue::TimeArray(times) => times[0],
            NormalValue::NillableTimeElementArray(times) => times[0].unwrap(),
            other => panic!("{kind:?}: {other:?}"),
        };
        assert!(
            time >= before && time <= chrono::Utc::now(),
            "{kind:?}: {time}"
        );
        assert_eq!(time.offset().local_minus_utc(), 0);
    }
}

#[test]
fn mutation_output_renders_time_as_go_rfc3339_nano() {
    let whole = DateTime::parse_from_rfc3339("2031-07-23T03:23:23Z").unwrap();
    let fraction = DateTime::parse_from_rfc3339("2031-07-23T03:23:23.120Z").unwrap();
    assert_eq!(
        normal_value_to_json(&NormalValue::Time(whole)),
        json!("2031-07-23T03:23:23Z")
    );
    assert_eq!(
        normal_value_to_json(&NormalValue::Time(fraction)),
        json!("2031-07-23T03:23:23.12Z")
    );
    assert_eq!(
        normal_value_to_json(&NormalValue::TimeArray(vec![whole])),
        json!(["2031-07-23T03:23:23Z"])
    );
}

#[test]
fn mutation_output_renders_float32_as_go() {
    assert_eq!(normal_value_to_json(&NormalValue::Float32(1.2)), json!(1.2));
    assert_eq!(
        normal_value_to_json(&NormalValue::Float32Array(vec![1.2, 3.3])),
        json!([1.2, 3.3])
    );
}
