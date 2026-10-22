//! Contract checks for a [`FieldCrdt`](super::FieldCrdt), for the built-in
//! types and for any type added later.

use document::stored::StoredDocument;
use document::NormalValue;
use schema::FieldDescription;

use super::{DeltaParts, FieldCrdt};
use defra_core::block::CrdtDelta;

fn delta_data(delta: &CrdtDelta) -> &[u8] {
    match delta {
        CrdtDelta::Lww(payload) => &payload.data,
        CrdtDelta::Counter(payload) => &payload.data,
        other => panic!("field delta expected, got {other:?}"),
    }
}

/// Check `crdt` against the laws of the write side, on `field`, for every
/// sample value:
///
/// - the block delta hands back the data it was given;
/// - the value a local write materializes to equals the operand the write
///   merges into the stored value;
/// - the type's identity operation, if it has one, changes nothing.
///
/// Panics on the first law that fails.
pub fn check_field_crdt(
    crdt: &dyn FieldCrdt,
    field: &FieldDescription,
    stored: &[NormalValue],
    inputs: &[NormalValue],
) {
    for input in inputs {
        let op = crdt.input_op(input.clone());
        crdt.check(&op, true).expect("a first write is accepted");

        let data = crdt.block_data(input, Some(&op));
        let bytes = format!("{data:?}").into_bytes();
        let delta = crdt.delta(DeltaParts {
            field_name: &field.name,
            priority: 7,
            schema_version_id: "version",
            nonce: 0,
            data: bytes.clone(),
        });
        assert_eq!(
            delta_data(&delta),
            bytes.as_slice(),
            "delta data round trip"
        );

        for stored in stored {
            let written = crdt
                .apply(field, Some(stored), &op)
                .expect("a sample op applies");
            let base = crdt.stored_field(field, 1, stored).expect("stored value");
            let operand_value = crdt
                .operand(&written, Some(&op))
                .expect("a local write has an operand form");
            let operand = crdt
                .stored_field(field, 2, operand_value)
                .expect("stored operand");
            let mut document = StoredDocument::default();
            document.fields.insert(0, base);
            let mut operands = StoredDocument::default();
            operands.fields.insert(0, operand);
            document.apply(operands).expect("operand merges");
            let merged = document.fields[&0].materialize();
            assert!(
                merged.as_ref() == Some(&written),
                "write {op:?} on {stored:?}: materialized {written:?}, operand merge gave {merged:?}"
            );
        }
    }

    if let Some(identity) = crdt.identity(field) {
        for stored in stored {
            let after = crdt
                .apply(field, Some(stored), &identity)
                .expect("identity applies");
            assert_eq!(&after, stored, "identity changes {stored:?}");
        }
    }
}
