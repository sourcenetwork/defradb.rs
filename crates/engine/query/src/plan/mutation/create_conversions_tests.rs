//! A `[T!]` field refuses a null element on create and update, as Go does,
//! instead of storing a zero value.

use schema::{FieldKind, ScalarArrayKind};
use serde_json::json;

use super::json_to_normal_value_with_kind;

#[test]
fn non_nillable_scalar_array_rejects_null_element() {
    for kind in [
        ScalarArrayKind::BoolArray,
        ScalarArrayKind::IntArray,
        ScalarArrayKind::Float64Array,
        ScalarArrayKind::Float32Array,
        ScalarArrayKind::StringArray,
    ] {
        let error =
            json_to_normal_value_with_kind(&json!([null]), Some(&FieldKind::ScalarArray(kind)))
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "null value provided for non-nillable field",
            "{kind:?}"
        );
    }
}

#[test]
fn nillable_scalar_array_keeps_null_element() {
    let value = json_to_normal_value_with_kind(
        &json!([1, null, 3]),
        Some(&FieldKind::ScalarArray(ScalarArrayKind::NillableIntArray)),
    )
    .unwrap();
    assert_eq!(
        value,
        document::NormalValue::NillableIntElementArray(vec![Some(1), None, Some(3)])
    );
}
