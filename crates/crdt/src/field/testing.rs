//! Contract checks for a [`FieldCrdt`](super::FieldCrdt), for the built-in
//! types and for any type added later.

use document::NormalValue;
use schema::FieldDescription;

use super::{DeltaParts, FieldCrdt};
use crate::stored::{StoredDocument, StoredField};
use defra_core::block::CrdtDelta;

fn delta_data(delta: &CrdtDelta) -> &[u8] {
    delta
        .field()
        .unwrap_or_else(|| panic!("field delta expected, got {delta:?}"))
        .data
}

/// `states` merged in order into the starting state, as the document value
/// folds operands.
fn fold<'a>(states: impl IntoIterator<Item = &'a StoredField>) -> StoredDocument {
    let mut document = StoredDocument::default();
    for state in states {
        let mut operand = StoredDocument::default();
        operand.fields.insert(0, state.clone());
        document.apply(operand).expect("a state merges");
    }
    document
}

fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    (0..items.len())
        .flat_map(|first| {
            let mut rest = items.to_vec();
            let head = rest.remove(first);
            permutations(&rest).into_iter().map(move |mut tail| {
                tail.insert(0, head.clone());
                tail
            })
        })
        .collect()
}

/// Check `crdt` against the laws of a field CRDT, on `field`, for every
/// sample: `stored` values a field holds, `inputs` a user writes.
///
/// Writes:
/// - the block delta hands back the data it was given;
/// - the value a local write materializes to equals the operand merge of the
///   write into the stored value;
/// - a stored value reads back as itself.
///
/// Blocks and merges:
/// - `decode(encode(x)) == x` for the data a write puts in its block;
/// - a block's state is the state of the value it carries;
/// - merging is commutative, and, where the type folds exactly, gives the
///   same state in any order and a partial merge of two operands equals
///   merging both into the base one by one;
/// - a state-based type's merge is idempotent and picks the same winner in
///   any order.
///
/// Panics on the first law that fails.
pub fn check_field_crdt(
    crdt: &dyn FieldCrdt,
    field: &FieldDescription,
    stored: &[NormalValue],
    inputs: &[NormalValue],
) {
    for value in stored {
        let state = crdt.state(field, 1, value).expect("stored value");
        assert_eq!(
            crdt.materialize(state.part()).as_ref(),
            Some(value),
            "stored {value:?} reads back"
        );
    }

    let mut block_states = Vec::new();
    for input in inputs {
        let op = crdt
            .op(input.clone(), true)
            .expect("a first write is accepted");
        let data = crdt.write(input, Some(&op)).data;
        let bytes = crdt.encode(data).expect("block data encodes");
        assert_eq!(
            &crdt.decode(field, &bytes).expect("block data decodes"),
            data,
            "decode(encode({data:?}))"
        );
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

        for priority in [1, 2] {
            let block = crdt
                .block_state(field, priority, &bytes)
                .expect("block state");
            let carried = crdt.state(field, priority, data).expect("carried state");
            assert_eq!(
                crdt.materialize(block.part()),
                crdt.materialize(carried.part()),
                "block of {data:?} at {priority} merges in the state it carries"
            );
            block_states.push(block);
        }

        for stored in stored {
            let written = crdt
                .apply(field, Some(stored), &op)
                .expect("a sample op applies");
            let write = crdt.write(&written, Some(&op));
            assert!(write.merges, "a local write of {op:?} merges as an operand");
            let base = crdt.state(field, 1, stored).expect("stored value");
            let operand = crdt.state(field, 2, write.data).expect("stored operand");
            let merged = fold([&base, &operand]);
            let merged = crdt.materialize(merged.fields[&0].part());
            assert!(
                merged.as_ref() == Some(&written),
                "write {op:?} on {stored:?}: materialized {written:?}, operand merge gave {merged:?}"
            );
        }
    }

    check_merges(crdt, &block_states);
}

fn check_merges(crdt: &dyn FieldCrdt, states: &[StoredField]) {
    for left in states {
        for right in states {
            assert_eq!(
                fold([left, right]),
                fold([right, left]),
                "merge of {left:?} and {right:?} commutes"
            );
            if !crdt.replicates_ops() {
                assert_eq!(
                    fold([left, right, right]),
                    fold([left, right]),
                    "merging {right:?} twice changes nothing"
                );
            }
            let exact = crdt.folds_exactly(left.part()) && crdt.folds_exactly(right.part());
            if !exact {
                continue;
            }
            let mut pair = left.clone();
            crdt.merge(&mut pair, right.clone()).expect("operands fold");
            for base in states {
                assert_eq!(
                    fold([base, &pair]),
                    fold([base, left, right]),
                    "partial merge of {left:?} and {right:?} onto {base:?}"
                );
            }
        }
    }
    if states.iter().all(|state| crdt.folds_exactly(state.part())) {
        let expected = fold(states);
        for order in permutations(states) {
            assert_eq!(fold(&order), expected, "merge order {order:?}");
        }
    }
}
