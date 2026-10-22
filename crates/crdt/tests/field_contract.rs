use crdt::field::testing::check_field_crdt;
use crdt::{delta_crdt, field_crdt, FieldCrdt};
use defra_core::block::CrdtDelta;
use defra_core::block_delta::{CounterDeltaPayload, LwwDeltaPayload, TextDeltaPayload};
use document::{FieldOp, NormalValue};
use proptest::prelude::*;
use schema::{CType, FieldDescription, FieldKind};

fn field(kind: FieldKind, ctype: CType) -> FieldDescription {
    FieldDescription::new("1", "value", kind, ctype)
}

fn crdt(ctype: CType) -> &'static dyn FieldCrdt {
    field_crdt(ctype).unwrap()
}

#[test]
fn lww_register_meets_the_contract() {
    check_field_crdt(
        crdt(CType::LwwRegister),
        &field(FieldKind::string(), CType::LwwRegister),
        &[NormalValue::String("a".into()), NormalValue::Null],
        &[
            NormalValue::String("b".into()),
            NormalValue::String("a".into()),
            NormalValue::Null,
        ],
    );
}

#[test]
fn counters_meet_the_contract() {
    for ctype in [CType::PCounter, CType::PnCounter] {
        check_field_crdt(
            crdt(ctype),
            &field(FieldKind::int(), ctype),
            &[NormalValue::Int(0), NormalValue::Int(41)],
            &[NormalValue::Int(1), NormalValue::Int(i64::MAX)],
        );
        for (kind, input) in [
            (FieldKind::float64(), NormalValue::Float64(2.25)),
            (FieldKind::float32(), NormalValue::Float32(2.25)),
        ] {
            check_field_crdt(
                crdt(ctype),
                &field(kind, ctype),
                &[NormalValue::Float64(0.5)],
                &[input],
            );
        }
    }
    check_field_crdt(
        crdt(CType::PnCounter),
        &field(FieldKind::int(), CType::PnCounter),
        &[NormalValue::Int(i64::MIN)],
        &[NormalValue::Int(-1), NormalValue::Int(7)],
    );
}

fn strings(values: &[&str]) -> Vec<NormalValue> {
    values
        .iter()
        .map(|value| NormalValue::String((*value).into()))
        .collect()
}

#[test]
fn text_meets_the_contract() {
    check_field_crdt(
        crdt(CType::Text),
        &field(FieldKind::string(), CType::Text),
        &[NormalValue::String("hello".into()), NormalValue::Null],
        &strings(&["hello world", "", "héllo", "help"]),
    );
}

#[test]
fn only_the_positive_counter_refuses_a_negative_value() {
    let negative = |ctype, creating| crdt(ctype).op(NormalValue::Int(-1), creating);
    assert!(negative(CType::PCounter, false).is_err());
    assert!(negative(CType::PnCounter, false).is_ok());
    assert!(negative(CType::PCounter, true).is_err());
    assert!(negative(CType::PnCounter, true).is_ok());
    let message = negative(CType::PCounter, true).unwrap_err().to_string();
    assert!(message.contains("value cannot be negative"), "{message}");
}

/// Go's `validateAndIncrement` adds with Go's `+`, which wraps on overflow.
#[test]
fn an_integer_counter_wraps_on_overflow() {
    let counter = crdt(CType::PnCounter);
    let field = field(FieldKind::int(), CType::PnCounter);
    let sum = counter
        .apply(
            &field,
            Some(&NormalValue::Int(i64::MAX)),
            &FieldOp::Increment(NormalValue::Int(1)),
        )
        .unwrap();
    assert_eq!(sum, NormalValue::Int(i64::MIN));
}

/// Go adds `Float32` increments as `float32`: the sum is rounded to `f32`
/// after every increment, never accumulated at `f64` precision.
#[test]
fn a_float32_counter_adds_at_f32_precision() {
    let counter = crdt(CType::PnCounter);
    let field = field(FieldKind::float32(), CType::PnCounter);
    let tenth = counter
        .decode(&field, &counter.encode(&NormalValue::Float32(0.1)).unwrap())
        .unwrap();
    let mut state = counter
        .state(&field, 1, &NormalValue::Float64(0.0))
        .unwrap();
    for _ in 0..3 {
        let increment = counter.state(&field, 1, &tenth).unwrap();
        counter.merge(&mut state, increment).unwrap();
    }
    let expected = 0.1f32 + 0.1f32 + 0.1f32;
    assert_eq!(
        counter.materialize(state.part()),
        Some(NormalValue::Float64(f64::from(expected)))
    );
}

#[test]
fn types_without_field_blocks_have_no_crdt() {
    assert!(field_crdt(CType::None).is_none());
    assert!(field_crdt(CType::Composite).is_none());
    assert!(field_crdt(CType::Unknown(9)).is_none());
}

#[test]
fn a_block_finds_the_crdt_that_wrote_it() {
    let lww = CrdtDelta::Lww(LwwDeltaPayload {
        field_name: "value".into(),
        priority: 1,
        collection_version_id: "v".into(),
        data: Vec::new(),
    });
    let counter = CrdtDelta::Counter(CounterDeltaPayload {
        field_name: "value".into(),
        priority: 1,
        nonce: 0,
        collection_version_id: "v".into(),
        data: Vec::new(),
    });
    let text = text();
    let ctype = |delta, declared| delta_crdt(delta, declared).ok().map(|crdt| crdt.ctype());
    assert_eq!(ctype(&lww, CType::LwwRegister), Some(CType::LwwRegister));
    assert_eq!(ctype(&lww, CType::None), Some(CType::LwwRegister));
    assert_eq!(ctype(&text, CType::Text), Some(CType::Text));
    assert_eq!(ctype(&text, CType::None), Some(CType::Text));
    assert_eq!(ctype(&counter, CType::PnCounter), Some(CType::PnCounter));
    assert_eq!(ctype(&counter, CType::PCounter), Some(CType::PCounter));
    assert_eq!(ctype(&counter, CType::None), Some(CType::PCounter));
}

/// A block of another family than its field's declared type is refused both
/// ways, naming the field, the declared type and the block's.
#[test]
fn a_block_of_another_family_is_refused() {
    let lww = CrdtDelta::Lww(LwwDeltaPayload {
        field_name: "value".into(),
        priority: 1,
        collection_version_id: "v".into(),
        data: Vec::new(),
    });
    let counter = CrdtDelta::Counter(CounterDeltaPayload {
        field_name: "value".into(),
        priority: 1,
        nonce: 0,
        collection_version_id: "v".into(),
        data: Vec::new(),
    });
    let text = text();
    let refusal = |delta, declared| match delta_crdt(delta, declared) {
        Err(error) => error.to_string(),
        Ok(crdt) => panic!("{declared} accepted a {} block", crdt.ctype()),
    };
    assert_eq!(
        refusal(&counter, CType::LwwRegister),
        "field 'value' is declared LwwRegister, but its block is of type counter"
    );
    assert_eq!(
        refusal(&lww, CType::PnCounter),
        "field 'value' is declared PnCounter, but its block is of type LWW"
    );
    assert_eq!(
        refusal(&lww, CType::PCounter),
        "field 'value' is declared PCounter, but its block is of type LWW"
    );
    for declared in [CType::LwwRegister, CType::PCounter, CType::PnCounter] {
        assert_eq!(
            refusal(&text, declared),
            format!("field 'value' is declared {declared}, but its block is of type text")
        );
    }
    assert_eq!(
        refusal(&lww, CType::Text),
        "field 'value' is declared Text, but its block is of type LWW"
    );
    assert_eq!(
        refusal(&counter, CType::Text),
        "field 'value' is declared Text, but its block is of type counter"
    );
}

fn text() -> CrdtDelta {
    CrdtDelta::Text(TextDeltaPayload {
        field_name: "value".into(),
        priority: 1,
        collection_version_id: "v".into(),
        data: Vec::new(),
    })
}

fn lww_value() -> impl Strategy<Value = NormalValue> {
    prop_oneof![
        Just(NormalValue::Null),
        "[a-c]{0,3}".prop_map(NormalValue::String),
        (-3i64..3).prop_map(NormalValue::Int),
    ]
}

proptest! {
    #[test]
    fn lww_merge_laws_hold(
        stored in proptest::collection::vec(lww_value(), 1..3),
        inputs in proptest::collection::vec(lww_value(), 1..3),
    ) {
        check_field_crdt(
            crdt(CType::LwwRegister),
            &field(FieldKind::string(), CType::LwwRegister),
            &stored,
            &inputs,
        );
    }

    #[test]
    fn text_merge_laws_hold(
        stored in proptest::collection::vec("[a-c]{0,3}", 1..3),
        inputs in proptest::collection::vec("[a-cé]{0,4}", 1..4),
    ) {
        let strings = |values: Vec<String>| values.into_iter().map(NormalValue::String).collect::<Vec<_>>();
        check_field_crdt(
            crdt(CType::Text),
            &field(FieldKind::string(), CType::Text),
            &strings(stored),
            &strings(inputs),
        );
    }

    #[test]
    fn integer_counter_merge_laws_hold(
        stored in proptest::collection::vec(any::<i64>(), 1..3),
        inputs in proptest::collection::vec(any::<i64>(), 1..3),
    ) {
        let ints = |values: Vec<i64>| values.into_iter().map(NormalValue::Int).collect::<Vec<_>>();
        check_field_crdt(
            crdt(CType::PnCounter),
            &field(FieldKind::int(), CType::PnCounter),
            &ints(stored),
            &ints(inputs),
        );
    }
}

#[test]
fn lww_block_state_refuses_bytes_that_are_not_one_cbor_value() {
    let lww = crdt(CType::LwwRegister);
    let field = field(FieldKind::string(), CType::LwwRegister);
    assert!(lww.block_state(&field, 1, &[0xff]).is_err());
    assert!(lww.block_state(&field, 1, &[0x61, b'a', 0x00]).is_err());
    assert!(lww.block_state(&field, 1, &[0x61, b'a']).is_ok());
    assert!(lww.block_state(&field, 1, &[]).is_ok());
}

/// Two `Document::edit` calls with `FieldOp::Edit` on one Text field stack
/// their edits in order: the second counts its positions in the text the first left.
#[test]
fn text_edits_in_one_write_stack_in_order() {
    let text = field(FieldKind::string(), CType::Text);
    let collection = schema::CollectionBuilder::new("Test", "bafytest")
        .field(text.clone())
        .build();
    let insert = |pos, text: &str| defra_core::TextOp::Insert {
        pos,
        text: text.into(),
    };
    let mut doc = document::Document::new(collection);
    doc.edit("value", FieldOp::Edit(vec![insert(5, " world")]))
        .unwrap();
    doc.edit("value", FieldOp::Edit(vec![insert(11, "!")]))
        .unwrap();
    let pending = doc.pending_op("value").unwrap();
    assert_eq!(
        pending,
        &FieldOp::Edit(vec![insert(5, " world"), insert(11, "!")])
    );
    assert_eq!(
        crdt(CType::Text)
            .apply(&text, Some(&NormalValue::String("hello".into())), pending)
            .unwrap(),
        NormalValue::String("hello world!".into())
    );
    doc.edit("value", FieldOp::Set(NormalValue::String("reset".into())))
        .unwrap();
    assert_eq!(doc.pending_op("value"), None, "a set drops the edits");
}

#[test]
fn only_edits_stack() {
    let one = NormalValue::Int(1);
    assert_eq!(
        FieldOp::Increment(one.clone()).then(FieldOp::Increment(NormalValue::Int(2))),
        FieldOp::Increment(NormalValue::Int(2))
    );
    assert_eq!(
        FieldOp::Edit(Vec::new()).then(FieldOp::Increment(one.clone())),
        FieldOp::Increment(one)
    );
}

/// A client names a set on a register or a text, an increment on a counter
/// and edits on a text; anything else is refused by the field's type.
#[test]
fn a_named_op_is_taken_only_by_a_type_that_takes_it() {
    let set = || FieldOp::Set(NormalValue::Int(1));
    let increment = |by| FieldOp::Increment(NormalValue::Int(by));
    let edit = || FieldOp::Edit(Vec::new());
    let named = |ctype, op| {
        field_crdt(ctype)
            .expect("a field type")
            .named_op(op)
            .map_err(|error| error.to_string())
    };
    assert_eq!(named(CType::LwwRegister, set()), Ok(set()));
    assert_eq!(named(CType::Text, set()), Ok(set()));
    assert_eq!(named(CType::Text, edit()), Ok(edit()));
    assert_eq!(named(CType::PnCounter, increment(-2)), Ok(increment(-2)));
    assert_eq!(named(CType::PCounter, increment(2)), Ok(increment(2)));
    assert_eq!(
        named(CType::PCounter, increment(-2)),
        Err("value cannot be negative".into())
    );
    for counter in [CType::PCounter, CType::PnCounter] {
        assert!(named(counter, set())
            .unwrap_err()
            .contains("use `increment`"));
        assert!(named(counter, edit()).is_err());
    }
    for (ctype, op) in [
        (CType::LwwRegister, increment(1)),
        (CType::LwwRegister, edit()),
        (CType::Text, increment(1)),
    ] {
        assert!(named(ctype, op).is_err());
    }
}

/// A Text write of null is the block data CBOR null, and a replay reads null
/// exactly while every head of the history writes null.
#[test]
fn a_text_history_whose_heads_write_null_replays_to_null() {
    let text = field(FieldKind::string(), CType::Text);
    let crdt = crdt(CType::Text);
    let causal = crdt.causal().unwrap();
    let nil = crdt
        .block_data(&NormalValue::Null, Some(&FieldOp::Set(NormalValue::Null)))
        .unwrap()
        .unwrap();
    assert_eq!(nil, [0xf6]);
    let block = |id: u8, parents: &[u8], data: &[u8]| crdt::CausalBlock {
        id: vec![id],
        parents: parents.iter().map(|parent| vec![*parent]).collect(),
        op: causal.block_op(&text, data).unwrap(),
    };
    let typed = crdt
        .block_data(&NormalValue::String("ab".into()), None)
        .unwrap()
        .unwrap();
    let first = block(1, &[], &typed);
    let replay = |history: &[crdt::CausalBlock]| causal.replay(&text, history).unwrap();

    assert_eq!(
        replay(&[first.clone(), block(2, &[1], &nil)]),
        NormalValue::Null
    );
    assert_eq!(
        replay(&[first.clone(), block(2, &[1], &nil), block(3, &[1], &nil)]),
        NormalValue::Null
    );
    assert_eq!(
        replay(&[first.clone(), block(2, &[1], &nil), block(3, &[1], &typed)]),
        NormalValue::String("ab".into())
    );
    assert_eq!(
        replay(&[first, block(2, &[1], &nil), block(3, &[2], &typed)]),
        NormalValue::String("ab".into())
    );
}
