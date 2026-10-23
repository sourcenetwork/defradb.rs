use crdt::field::testing::check_field_crdt;
use crdt::{Crdts, FieldCrdt, FieldCrdtError, JsonField};
use defra_core::block::CrdtDelta;
use defra_core::block_delta::{JsonDeltaPayload, LwwDeltaPayload, TextDeltaPayload};
use defra_core::json_crdt::{JsonEdit, JsonOp};
use document::{FieldOp, NormalValue};
use proptest::prelude::*;
use schema::{CType, FieldDescription, FieldKind};
use serde_json::{json, Value};

fn field() -> FieldDescription {
    FieldDescription::new("1", "meta", FieldKind::json(), CType::Json)
}

fn json_crdt() -> &'static dyn FieldCrdt {
    crdt::field::native(CType::Json).unwrap()
}

fn values(values: &[Value]) -> Vec<NormalValue> {
    values.iter().cloned().map(NormalValue::Json).collect()
}

#[test]
fn json_meets_the_contract() {
    check_field_crdt(
        json_crdt(),
        &field(),
        &values(&[json!({"a": 1, "b": {"c": 2}}), json!(5)]),
        &values(&[
            json!({"a": 2, "b": {"c": 2}}),
            json!({"b": {"d": [1, 2]}}),
            json!("text"),
        ]),
    );
}

/// The ID a Json block's operation takes is #2150's block ID, byte for byte,
/// so data written before Json became a tagged type still merges.
#[test]
fn a_json_op_id_is_the_block_id_json_blocks_were_written_with() {
    let tagged = json_crdt().tagged().expect("Json is tagged");
    let hex = |bytes: Vec<u8>| -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() };
    assert_eq!(
        hex(tagged.op_id(3, &[vec![1, 0x71, 0x12], vec![1, 0x55]], b"json ops")),
        "6a4275e21a9b038a024aa1c1db0ff1f2b82ceb3ebf5a55e7c3e2b08bda48d46a"
    );
    assert_eq!(
        hex(tagged.op_id(1, &[], b"")),
        "9e3d2c0f8d5e3c41b566eba970a28524fb384c5fd714e37e168c0dedefd020ee"
    );
}

#[test]
fn json_is_tagged_and_not_causal() {
    assert!(json_crdt().tagged().is_some());
    assert!(json_crdt().causal().is_none());
}

fn json_block() -> CrdtDelta {
    CrdtDelta::Json(JsonDeltaPayload {
        field_name: "meta".into(),
        priority: 1,
        schema_version_id: "v".into(),
        data: Vec::new(),
    })
}

/// A Json block is read only for a Json field, and a Json field reads no
/// block of another family.
#[test]
fn a_json_block_is_refused_like_any_other_family() {
    let refusal = |delta: &CrdtDelta, declared| match delta_crdt(delta, declared) {
        Err(error) => error.to_string(),
        Ok(ctype) => panic!("{declared} accepted a {} block", ctype),
    };
    let json = json_block();
    for declared in [
        CType::LwwRegister,
        CType::PCounter,
        CType::PnCounter,
        CType::Text,
    ] {
        assert_eq!(
            refusal(&json, declared),
            format!("field 'meta' is declared {declared}, but its block is of type json")
        );
    }
    for declared in [CType::Json, CType::None] {
        assert_eq!(delta_crdt(&json, declared).ok(), Some(CType::Json));
    }
    let lww = CrdtDelta::Lww(LwwDeltaPayload {
        field_name: "meta".into(),
        priority: 1,
        schema_version_id: "v".into(),
        data: Vec::new(),
    });
    let text = CrdtDelta::Text(TextDeltaPayload {
        field_name: "meta".into(),
        priority: 1,
        schema_version_id: "v".into(),
        data: Vec::new(),
    });
    assert_eq!(
        refusal(&lww, CType::Json),
        "field 'meta' is declared Json, but its block is of type LWW"
    );
    assert_eq!(
        refusal(&text, CType::Json),
        "field 'meta' is declared Json, but its block is of type text"
    );
}

/// A named set on a Json field sets each leaf at its path and leaves every
/// other key alone; a written value replaces the field's; a path delete
/// removes one path. Only Json takes path edits.
#[test]
fn a_json_field_takes_sets_and_path_edits() {
    let json = json_crdt();
    let field = field();
    let current = NormalValue::Json(json!({"a": 1, "b": {"c": 2}, "s": 5}));
    let named = json
        .named_op(FieldOp::Set(NormalValue::Json(
            json!({"b": {"d": 3}, "s": {"t": 1}}),
        )))
        .unwrap();
    assert_eq!(
        json.apply(&field, Some(&current), &named).unwrap(),
        NormalValue::Json(json!({"a": 1, "b": {"c": 2, "d": 3}, "s": {"t": 1}}))
    );
    let replaced = json.op(NormalValue::Json(json!({"z": 0})), false).unwrap();
    assert_eq!(
        json.apply(&field, Some(&current), &replaced).unwrap(),
        NormalValue::Json(json!({"z": 0}))
    );
    let delete = FieldOp::Paths(vec![JsonOp {
        path: vec!["b".into(), "c".into()],
        edit: JsonEdit::Delete,
    }]);
    assert_eq!(json.named_op(delete.clone()).unwrap(), delete);
    assert_eq!(
        json.apply(&field, Some(&current), &delete).unwrap(),
        NormalValue::Json(json!({"a": 1, "b": {}, "s": 5}))
    );
    for op in [
        FieldOp::Increment(NormalValue::Int(1)),
        FieldOp::Edit(Vec::new()),
    ] {
        assert!(json.named_op(op).is_err());
    }
    for ctype in [CType::LwwRegister, CType::Text, CType::PnCounter] {
        let message = crdt::field::native(ctype)
            .unwrap()
            .named_op(delete.clone())
            .unwrap_err()
            .to_string();
        assert!(message.contains("do not support path edits"), "{message}");
    }
}

#[test]
fn only_a_json_field_has_conflicts() {
    let lww = FieldDescription::new("1", "meta", FieldKind::string(), CType::LwwRegister);
    let message = JsonField::conflicts(&lww, None).unwrap_err().to_string();
    assert_eq!(
        message,
        "LwwRegister fields do not support conflict queries"
    );
    assert_eq!(JsonField::conflicts(&field(), None).unwrap(), Vec::new());
}

fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        (0i64..3).prop_map(Value::from),
        "[a-b]{0,2}".prop_map(Value::from),
        proptest::collection::vec((0i64..2).prop_map(Value::from), 0..2).prop_map(Value::from),
    ];
    leaf.prop_recursive(2, 6, 3, |inner| {
        proptest::collection::btree_map("[a-c]", inner, 0..3)
            .prop_map(|map| Value::Object(map.into_iter().collect()))
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn json_merge_laws_hold(
        stored in proptest::collection::vec(json_value(), 1..3),
        inputs in proptest::collection::vec(json_value(), 1..4),
    ) {
        check_field_crdt(json_crdt(), &field(), &values(&stored), &values(&inputs));
    }
}

/// A write of a map over a value whose map is only implied by a concurrent
/// edit below it (`{"a": null}` racing `""`) still reads as the map written.
#[test]
fn a_map_written_over_an_implied_map_reads_as_written() {
    check_field_crdt(
        json_crdt(),
        &field(),
        &[NormalValue::Null],
        &values(&[json!({}), json!({"a": null}), json!("")]),
    );
}

/// Two path edits of one Json field in one write stack in order, as #2150's
/// edit_json calls did, and a later set drops them.
#[test]
fn json_path_edits_in_one_write_stack_in_order() {
    let collection = schema::CollectionBuilder::new("Test", "bafytest")
        .field(field())
        .build();
    let op = |path: &[&str], edit| JsonOp {
        path: path.iter().map(|key| key.to_string()).collect(),
        edit,
    };
    let mut doc = document::Document::new(collection);
    doc.edit("meta", FieldOp::Paths(vec![op(&["a"], JsonEdit::Delete)]))
        .unwrap();
    doc.edit(
        "meta",
        FieldOp::Paths(vec![op(&["b", "c"], JsonEdit::Set(json!(2)))]),
    )
    .unwrap();
    let pending = doc.pending_op("meta").unwrap();
    assert_eq!(
        pending,
        &FieldOp::Paths(vec![
            op(&["a"], JsonEdit::Delete),
            op(&["b", "c"], JsonEdit::Set(json!(2))),
        ])
    );
    assert_eq!(
        json_crdt()
            .apply(
                &field(),
                Some(&NormalValue::Json(json!({"a": 1, "b": {}}))),
                pending
            )
            .unwrap(),
        NormalValue::Json(json!({"b": {"c": 2}}))
    );
    doc.edit("meta", FieldOp::Set(NormalValue::Json(json!(1))))
        .unwrap();
    assert_eq!(doc.pending_op("meta"), None, "a set drops the edits");
}

/// The type a native node reads a field block as.
fn delta_crdt(delta: &CrdtDelta, declared: CType) -> Result<CType, FieldCrdtError> {
    Crdts::default()
        .delta(delta, declared)
        .map(|crdt| crdt.ctype())
}
