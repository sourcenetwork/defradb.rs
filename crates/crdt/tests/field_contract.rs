use crdt::field::testing::check_field_crdt;
use crdt::{delta_crdt, field_crdt, FieldCrdt};
use defra_core::block::CrdtDelta;
use defra_core::block_delta::{CounterDeltaPayload, LwwDeltaPayload};
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
        schema_version_id: "v".into(),
        data: Vec::new(),
    });
    let counter = CrdtDelta::Counter(CounterDeltaPayload {
        field_name: "value".into(),
        priority: 1,
        nonce: 0,
        schema_version_id: "v".into(),
        data: Vec::new(),
    });
    let ctype = |delta, declared| delta_crdt(delta, declared).ok().map(|crdt| crdt.ctype());
    assert_eq!(ctype(&lww, CType::LwwRegister), Some(CType::LwwRegister));
    assert_eq!(ctype(&lww, CType::None), Some(CType::LwwRegister));
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
        schema_version_id: "v".into(),
        data: Vec::new(),
    });
    let counter = CrdtDelta::Counter(CounterDeltaPayload {
        field_name: "value".into(),
        priority: 1,
        nonce: 0,
        schema_version_id: "v".into(),
        data: Vec::new(),
    });
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
