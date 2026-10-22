//! Contract checks for a [`FieldCrdt`](super::FieldCrdt), for the built-in
//! types and for any type added later.

use document::{FieldOp, NormalValue};
use schema::FieldDescription;

use super::{CausalBlock, CausalCrdt, DeltaParts, FieldCrdt};
use crate::stored::{StoredDocument, StoredField};
use defra_core::block::CrdtDelta;

/// `states` merged in order into the starting state, as the document value
/// folds operands.
fn fold<'state>(states: impl IntoIterator<Item = &'state StoredField>) -> StoredDocument {
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
/// A causal type ([`FieldCrdt::causal`]) stores a value that every writer
/// read validated, so its part merges replace in commit order instead, and
/// its replay obeys [`check_causal`] on histories built from `inputs`.
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
            nonce: 42,
            data: bytes.clone(),
        });
        let carried = delta
            .field()
            .unwrap_or_else(|| panic!("field delta expected, got {delta:?}"));
        assert_eq!(carried.field_name, field.name, "delta field name");
        assert_eq!(carried.priority, 7, "delta priority");
        assert_eq!(carried.schema_version_id, "version", "delta version id");
        assert_eq!(carried.data, bytes.as_slice(), "delta data round trip");
        match &delta {
            CrdtDelta::Counter(counter) => assert_eq!(counter.nonce, 42, "delta nonce"),
            // A causal op is told apart by the parents it counts in, so only
            // a blind op needs a nonce to repeat.
            _ => assert!(!crdt.blind_op(), "a blind operation delta carries a nonce"),
        }

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

    match crdt.causal() {
        Some(causal) => {
            check_replacements(crdt, &block_states);
            check_causal(crdt, causal, field, inputs);
        }
        None => check_merges(crdt, &block_states),
    }
}

/// A causal type's part merge: the later state replaces the earlier, in any
/// grouping.
fn check_replacements(crdt: &dyn FieldCrdt, states: &[StoredField]) {
    for left in states {
        for right in states {
            assert_eq!(
                fold([left, right]).fields[&0].materialize(),
                crdt.materialize(right.part()),
                "{right:?} replaces {left:?}"
            );
            let mut pair = left.clone();
            crdt.merge(&mut pair, right.clone()).expect("states fold");
            for base in states {
                assert_eq!(
                    fold([base, &pair]),
                    fold([base, left, right]),
                    "partial merge of {left:?} and {right:?} onto {base:?}"
                );
            }
        }
    }
}

fn block(id: u8, parents: &[u8], op: FieldOp) -> CausalBlock {
    CausalBlock {
        id: vec![id],
        parents: parents.iter().map(|&parent| vec![parent]).collect(),
        op,
    }
}

/// Check the replay of a causal type on histories of `values`, each the
/// value of a write; nil values are skipped. For a first write `base`, and writes `a` and `b` made
/// concurrently on top of it:
///
/// - a block's data hands back the operation written in it;
/// - a first write's history replays to its value;
/// - `apply(current, diff(current, value)) == value`;
/// - a block whose parents are the history's heads replays to its operation
///   applied to the history's value, the merge's shortcut;
/// - the same blocks in any order replay to the same value;
/// - a block that restates a value replays to that value.
///
/// Panics on the first law that fails.
pub fn check_causal(
    crdt: &dyn FieldCrdt,
    causal: &dyn CausalCrdt,
    field: &FieldDescription,
    values: &[NormalValue],
) {
    let replay = |history: &[CausalBlock]| causal.replay(field, history).expect("history replays");
    let block_op = |value: &NormalValue, op: Option<&FieldOp>| {
        let data = crdt
            .block_data(value, op)
            .expect("block data encodes")
            .expect("a write of a value makes a block");
        causal.block_op(field, &data).expect("block data decodes")
    };
    let diff = |current: &NormalValue, value: &NormalValue| {
        let op = causal.diff(Some(current), value).expect("a diff");
        assert_eq!(
            &crdt
                .apply(field, Some(current), &op)
                .expect("a diff applies"),
            value,
            "diff({current:?}, {value:?}) applied"
        );
        assert_eq!(block_op(value, Some(&op)), op, "{op:?} round trips");
        op
    };
    let values: Vec<&NormalValue> = values.iter().filter(|value| !value.is_nil()).collect();
    for &base in &values {
        let first = block(0, &[], block_op(base, None));
        assert_eq!(
            &replay(std::slice::from_ref(&first)),
            base,
            "first write {base:?}"
        );
        for &a in &values {
            let left = block(1, &[0], diff(base, a));
            let linear = [first.clone(), left.clone()];
            assert_eq!(
                replay(&linear),
                crdt.apply(field, Some(base), &left.op).expect("applies"),
                "{a:?} on {base:?}: replay against the shortcut"
            );
            let restated = block(1, &[0], FieldOp::Set(a.clone()));
            assert_eq!(&replay(&[first.clone(), restated]), a, "{a:?} restated");
            for &b in &values {
                let right = block(2, &[0], diff(base, b));
                let concurrent = [first.clone(), left.clone(), right];
                let merged = replay(&concurrent);
                for order in permutations(&concurrent) {
                    assert_eq!(replay(&order), merged, "merge order {order:?}");
                }
                let next = block(3, &[1, 2], diff(&merged, base));
                let mut after = concurrent.to_vec();
                after.push(next.clone());
                assert_eq!(
                    &replay(&after),
                    base,
                    "a write on the merge of {a:?} and {b:?} against the shortcut"
                );
            }
        }
    }
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
