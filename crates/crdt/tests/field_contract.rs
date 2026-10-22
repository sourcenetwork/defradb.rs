use crdt::field::testing::check_field_crdt;
use crdt::field_crdt;
use document::NormalValue;
use schema::{CType, FieldDescription, FieldKind};

fn field(kind: FieldKind, ctype: CType) -> FieldDescription {
    FieldDescription::new("1", "value", kind, ctype)
}

#[test]
fn lww_register_meets_the_contract() {
    let crdt = field_crdt(CType::LwwRegister).unwrap();
    check_field_crdt(
        crdt,
        &field(FieldKind::string(), CType::LwwRegister),
        &[NormalValue::String("a".into()), NormalValue::Null],
        &[NormalValue::String("b".into()), NormalValue::Null],
    );
}

#[test]
fn counters_meet_the_contract() {
    for ctype in [CType::PCounter, CType::PnCounter] {
        let crdt = field_crdt(ctype).unwrap();
        check_field_crdt(
            crdt,
            &field(FieldKind::int(), ctype),
            &[NormalValue::Int(0), NormalValue::Int(41)],
            &[NormalValue::Int(1), NormalValue::Int(i64::MAX)],
        );
        check_field_crdt(
            crdt,
            &field(FieldKind::float64(), ctype),
            &[NormalValue::Float64(0.5)],
            &[NormalValue::Float64(2.25)],
        );
    }
}

#[test]
fn only_the_positive_counter_refuses_a_negative_update() {
    let negative = |ctype| {
        let crdt = field_crdt(ctype).unwrap();
        crdt.check(&crdt.input_op(NormalValue::Int(-1)), false)
    };
    assert!(negative(CType::PCounter).is_err());
    assert!(negative(CType::PnCounter).is_ok());
    let crdt = field_crdt(CType::PCounter).unwrap();
    assert!(crdt
        .check(&crdt.input_op(NormalValue::Int(-1)), true)
        .is_ok());
}

#[test]
fn types_without_field_blocks_have_no_crdt() {
    assert!(field_crdt(CType::None).is_none());
    assert!(field_crdt(CType::Composite).is_none());
    assert!(field_crdt(CType::Unknown(9)).is_none());
}
