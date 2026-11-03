//! A `Float32` field reads back as the `f32` it stored and renders as Go
//! renders a `float32`: `1.2`, not `1.2000000476837158` (#2360).

use document::encoding::{normal_value_to_json, retype_stored_float32, widen_f32};
use document::{Document, NormalValue};
use schema::{CType, CollectionBuilder, FieldDescription, FieldKind, ScalarArrayKind, ScalarKind};
use serde_json::json;

fn float32_collection() -> schema::CollectionVersion {
    CollectionBuilder::new("Test", "bafytest")
        .field(FieldDescription::new(
            "1",
            "score",
            FieldKind::Scalar(ScalarKind::Float32),
            CType::LwwRegister,
        ))
        .field(FieldDescription::new(
            "2",
            "scores",
            FieldKind::ScalarArray(ScalarArrayKind::Float32Array),
            CType::LwwRegister,
        ))
        .field(FieldDescription::new(
            "3",
            "maybe",
            FieldKind::ScalarArray(ScalarArrayKind::NillableFloat32Array),
            CType::LwwRegister,
        ))
        .build()
}

#[test]
fn widen_f32_renders_the_shortest_decimal() {
    assert_eq!(widen_f32(1.2), 1.2);
    assert_eq!(widen_f32(-3.4e-12), -3.4e-12);
    assert_eq!(widen_f32(f32::MAX), 3.4028235e38);
    assert_eq!(widen_f32(f32::MIN_POSITIVE), 1.1754944e-38);
    assert_eq!(widen_f32(0.0), 0.0);
}

#[test]
fn widen_f32_narrows_back_to_the_same_f32() {
    // Every 4099th bit pattern covers each exponent and sign, subnormals included.
    for bits in (0..=u32::MAX).step_by(4099) {
        let value = f32::from_bits(bits);
        if value.is_finite() {
            assert_eq!(
                (widen_f32(value) as f32).to_bits(),
                value.to_bits(),
                "{value}"
            );
        }
    }
}

#[test]
fn float32_values_render_as_go_renders_them() {
    assert_eq!(
        normal_value_to_json(&NormalValue::Float32(1.2)).unwrap(),
        json!(1.2)
    );
    assert_eq!(
        normal_value_to_json(&NormalValue::Float32Array(vec![1.2, -0.1])).unwrap(),
        json!([1.2, -0.1])
    );
    assert_eq!(
        normal_value_to_json(&NormalValue::NillableFloat32ElementArray(vec![
            None,
            Some(3.3)
        ]))
        .unwrap(),
        json!([null, 3.3])
    );
}

#[test]
fn a_stored_float32_document_reads_back_as_float32() {
    let collection = float32_collection();
    let mut written = Document::new(collection.clone());
    written.set("score", NormalValue::Float32(1.2)).unwrap();
    written
        .set("scores", NormalValue::Float32Array(vec![1.2, 2.5]))
        .unwrap();
    written
        .set(
            "maybe",
            NormalValue::NillableFloat32ElementArray(vec![None, Some(0.1)]),
        )
        .unwrap();
    let bytes = written.to_cbor().unwrap();

    let read = Document::from_cbor(&bytes, collection).unwrap();
    assert_eq!(read.get("score"), Some(&NormalValue::Float32(1.2)));
    assert_eq!(
        read.get("scores"),
        Some(&NormalValue::Float32Array(vec![1.2, 2.5]))
    );
    assert_eq!(
        read.get("maybe"),
        Some(&NormalValue::NillableFloat32ElementArray(vec![
            None,
            Some(0.1)
        ]))
    );
    assert_eq!(
        read.to_cbor().unwrap(),
        bytes,
        "re-encoding is byte-identical"
    );
}

#[test]
fn retype_leaves_other_kinds_and_values_alone() {
    let float64 = FieldKind::Scalar(ScalarKind::Float64);
    assert_eq!(
        retype_stored_float32(NormalValue::Float64(1.2), &float64),
        NormalValue::Float64(1.2)
    );
    let float32 = FieldKind::Scalar(ScalarKind::Float32);
    assert_eq!(
        retype_stored_float32(NormalValue::Int(3), &float32),
        NormalValue::Int(3)
    );
    let nillable = FieldKind::ScalarArray(ScalarArrayKind::NillableFloat32Array);
    let mixed = NormalValue::JsonArray(vec![json!(null), json!("x")]);
    assert_eq!(retype_stored_float32(mixed.clone(), &nillable), mixed);
}
