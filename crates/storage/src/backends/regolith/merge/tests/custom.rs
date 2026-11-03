//! Custom parts: the operator a node builds with its registered types folds
//! and touches parts of a custom kind under the engine's contract.

use crdt::field::LwwField;
use crdt::stored::{Part, StoredDocument, StoredField, ValueKind};
use crdt::{Crdts, DeltaParts, FieldCrdt, FieldCrdtError, FieldWrite, MergeResult, Registration};
use defra_core::{CrdtDelta, CustomDeltaPayload};
use document::{FieldOp, NormalValue};
use regolith::testing::proptest::prelude::*;
use regolith::MergeOperator;
use schema::{CType, CrdtName, FieldDescription};

use crate::merge::{document_parts, DefraMergeOperator};

const NAME: &str = "maxreg";

fn name() -> CrdtName {
    CrdtName::new(NAME).unwrap()
}

/// An integer field whose value is the greatest any replica wrote: a join,
/// so its parts fold exactly in any grouping.
struct MaxRegister;

fn value(bytes: &[u8]) -> Option<i64> {
    Some(i64::from_be_bytes(bytes.try_into().ok()?))
}

/// An empty state, as a fold starts a part from, ranks below every value.
fn rank(bytes: &[u8]) -> Option<i128> {
    match bytes {
        [] => Some(i128::MIN),
        bytes => value(bytes).map(i128::from),
    }
}

fn bytes(value: &NormalValue) -> Result<Vec<u8>, FieldCrdtError> {
    match value {
        NormalValue::Int(value) => Ok(value.to_be_bytes().to_vec()),
        other => Err(FieldCrdtError::InvalidInput {
            crdt: CType::Custom(name()),
            message: format!("{other:?}; a max register holds integers"),
        }),
    }
}

impl FieldCrdt for MaxRegister {
    fn ctype(&self) -> CType {
        CType::Custom(name())
    }

    fn replicates_ops(&self) -> bool {
        false
    }

    fn op(&self, input: NormalValue, _creating: bool) -> Result<FieldOp, FieldCrdtError> {
        Ok(FieldOp::Set(input))
    }

    fn apply(
        &self,
        _field: &FieldDescription,
        _current: Option<&NormalValue>,
        op: &FieldOp,
    ) -> Result<NormalValue, FieldCrdtError> {
        match op {
            FieldOp::Set(input) => Ok(input.clone()),
            other => Err(FieldCrdtError::UnsupportedOp {
                crdt: self.ctype(),
                op: other.name(),
            }),
        }
    }

    fn write<'value>(
        &self,
        value: &'value NormalValue,
        _op: Option<&'value FieldOp>,
    ) -> FieldWrite<'value> {
        FieldWrite {
            data: value,
            merges: true,
        }
    }

    fn block_data(
        &self,
        value: &NormalValue,
        _op: Option<&FieldOp>,
    ) -> Result<Option<Vec<u8>>, FieldCrdtError> {
        bytes(value).map(Some)
    }

    fn decode(&self, field: &FieldDescription, data: &[u8]) -> Result<NormalValue, FieldCrdtError> {
        value(data)
            .map(NormalValue::Int)
            .ok_or_else(|| FieldCrdtError::Decode {
                field: field.name.clone(),
                message: format!("{} bytes are not a max register", data.len()),
            })
    }

    fn delta(&self, parts: DeltaParts<'_>) -> CrdtDelta {
        CrdtDelta::Custom(CustomDeltaPayload {
            crdt: NAME.to_owned(),
            field_name: parts.field_name.to_owned(),
            priority: parts.priority,
            nonce: None,
            collection_version_id: parts.collection_version_id.to_owned(),
            data: parts.data,
        })
    }

    fn state(
        &self,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
    ) -> Result<StoredField, FieldCrdtError> {
        Ok(StoredField {
            name: field.name.clone(),
            kind: ValueKind::Custom(name()),
            priority,
            value: bytes(value)?,
        })
    }

    fn materialize(&self, state: Part<'_>) -> Option<NormalValue> {
        (state.kind == ValueKind::Custom(name()))
            .then(|| value(state.value).map(NormalValue::Int))
            .flatten()
    }

    fn merge(&self, state: &mut StoredField, incoming: StoredField) -> Option<MergeResult> {
        let current = (rank(&state.value)?, state.priority);
        let offered = (rank(&incoming.value)?, incoming.priority);
        if offered <= current {
            return Some(MergeResult::RejectedTieBreak);
        }
        *state = incoming;
        Some(MergeResult::Applied)
    }

    fn folds_exactly(&self, _state: Part<'_>) -> bool {
        true
    }
}

fn operator() -> DefraMergeOperator {
    DefraMergeOperator::new(Crdts::new([Registration::new(MaxRegister)]).unwrap())
}

fn register(id: u32, priority: u64, value: i64) -> StoredField {
    StoredField {
        name: format!("best{id}"),
        kind: ValueKind::Custom(name()),
        priority,
        value: value.to_be_bytes().to_vec(),
    }
}

/// Parts 0 to 2 are max registers, part 3 an LWW field.
fn documents() -> impl Strategy<Value = StoredDocument> + Clone {
    let part = (0u32..4, 1u64..4, -1000i64..1000).prop_map(|(id, priority, value)| {
        let field = if id < 3 {
            register(id, priority, value)
        } else {
            LwwField::stored(format!("field{id}"), priority, &NormalValue::Int(value)).unwrap()
        };
        (id, field)
    });
    regolith::testing::proptest::collection::vec(part, 1..3).prop_map(|parts| StoredDocument {
        fields: parts.into_iter().collect(),
    })
}

#[test]
fn registered_custom_parts_fold_and_touch_under_engine_contracts() {
    let operands = documents().prop_map(|value| document_parts(&value).unwrap());
    let checked = regolith::testing::check_merge_operator(
        &operator(),
        Just(b"doc".to_vec()),
        documents().prop_map(|value| value.encode().unwrap()),
        operands.clone(),
    )
    .unwrap();
    assert!(checked.folds > 0, "custom operands must fold");
    let checked = regolith::testing::check_touches(
        &operator(),
        Just(b"doc".to_vec()),
        operands,
        regolith::testing::proptest::collection::vec(0u32..5, 0..6),
    )
    .unwrap();
    assert!(checked.folds > 0);
}

/// Folding goes through the node's registry: an operator without the type
/// keeps the operands apart rather than guess.
#[test]
fn custom_parts_fold_only_with_their_type_registered() {
    let part = |value| {
        document_parts(&StoredDocument {
            fields: [(0, register(0, 1, value))].into(),
        })
        .unwrap()
    };
    let folded = operator().partial_merge(b"doc", &part(3), &part(7));
    assert_eq!(folded, Some(part(7)));
    assert_eq!(
        DefraMergeOperator::default().partial_merge(b"doc", &part(3), &part(7)),
        None
    );
}
