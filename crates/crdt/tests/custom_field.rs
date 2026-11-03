//! A custom CRDT type this node has not registered: its fields and blocks
//! are refused, naming the type, and never handled as another type.

use crdt::{Crdts, FieldCrdtError};
use defra_core::block::CrdtDelta;
use defra_core::block_delta::{CustomDeltaPayload, LwwDeltaPayload};
use document::NormalValue;
use schema::{CType, CrdtName};

fn absent() -> CType {
    CType::Custom(CrdtName::new("absent").unwrap())
}

fn custom(crdt: &str) -> CrdtDelta {
    CrdtDelta::Custom(CustomDeltaPayload {
        crdt: crdt.into(),
        field_name: "value".into(),
        priority: 1,
        nonce: None,
        collection_version_id: "v".into(),
        data: Vec::new(),
    })
}

fn lww() -> CrdtDelta {
    CrdtDelta::Lww(LwwDeltaPayload {
        field_name: "value".into(),
        priority: 1,
        collection_version_id: "v".into(),
        data: Vec::new(),
    })
}

fn refusal(delta: &CrdtDelta, declared: CType) -> FieldCrdtError {
    match Crdts::default().delta(delta, declared) {
        Err(error) => error,
        Ok(crdt) => panic!("{declared} accepted a block as {}", crdt.ctype()),
    }
}

#[test]
fn an_unregistered_field_refuses_every_write() {
    let crdts = Crdts::default();
    assert!(crdts.is_unregistered(absent()));
    let stand_in = crdts
        .field(absent())
        .expect("a custom field has a stand-in");
    assert_eq!(stand_in.ctype(), absent());
    let error = stand_in.op(NormalValue::Int(1), true).unwrap_err();
    assert_eq!(
        error.to_string(),
        "'absent' is a custom CRDT type this node does not have; register it with \
         NodeBuilder::with_crdt to write or merge its fields"
    );
}

#[test]
fn an_unregistered_block_is_refused_by_name() {
    for declared in [absent(), CType::None] {
        assert!(matches!(
            refusal(&custom("absent"), declared),
            FieldCrdtError::Unregistered { crdt } if crdt.as_str() == "absent"
        ));
    }
}

#[test]
fn a_custom_block_and_a_native_field_never_cross() {
    assert_eq!(
        refusal(&custom("absent"), CType::LwwRegister).to_string(),
        "field 'value' is declared LwwRegister, but its block is of type custom"
    );
    assert_eq!(
        refusal(&lww(), absent()).to_string(),
        "field 'value' is declared absent, but its block is of type LWW"
    );
    assert_eq!(
        refusal(&custom("other"), absent()).to_string(),
        "field 'value' is declared absent, but its block is of type custom"
    );
    assert!(matches!(
        refusal(&custom("lww"), CType::None),
        FieldCrdtError::MismatchedBlock { .. }
    ));
}
