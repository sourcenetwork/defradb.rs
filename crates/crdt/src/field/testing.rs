//! Contract checks for a [`FieldCrdt`](super::FieldCrdt), for the built-in
//! types and for any type added later.

use document::{FieldOp, NormalValue};
use schema::FieldDescription;

use super::tagged::{decode_operand, encode_operand, fold_history, land, local_write, LocalWrite};
use super::{CausalBlock, CausalCrdt, DeltaParts, FieldCrdt, TaggedBlock, TaggedCrdt};
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

fn permutations<T>(items: &[T]) -> Vec<Vec<T>>
where
    T: Clone,
{
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
/// its replay obeys [`check_causal`] on histories built from `inputs`. A
/// tagged type ([`FieldCrdt::tagged`]) writes against its stored state, so
/// its local writes are checked by [`check_tagged`] on histories built from
/// `inputs`. A type is at most one of the two.
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
            collection_version_id: "version",
            nonce: 42,
            data: bytes.clone(),
        });
        let carried = delta
            .field()
            .unwrap_or_else(|| panic!("field delta expected, got {delta:?}"));
        assert_eq!(carried.field_name, field.name, "delta field name");
        assert_eq!(carried.priority, 7, "delta priority");
        assert_eq!(carried.collection_version_id, "version", "delta version id");
        assert_eq!(carried.data, bytes.as_slice(), "delta data round trip");
        match &delta {
            CrdtDelta::Counter(counter) => assert_eq!(counter.nonce, 42, "delta nonce"),
            // A causal op is told apart by the parents it counts in, and a
            // tagged op by its ID, so only another blind op needs a nonce to
            // repeat.
            _ => assert!(
                !crdt.blind_op() || crdt.tagged().is_some(),
                "a blind operation delta carries a nonce"
            ),
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
            if let Some(tagged) = crdt.tagged() {
                let base = crdt.state(field, 1, stored).expect("stored value");
                let write = local_write(tagged, field, Some(&base), &op, 2, &[vec![0]], 0)
                    .expect("a local write prepares");
                assert_eq!(
                    write.value, written,
                    "write {op:?} on {stored:?}: the prepared write against apply"
                );
                continue;
            }
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

    match (crdt.causal(), crdt.tagged()) {
        (Some(_), Some(_)) => panic!("a type is at most one of causal and tagged"),
        (Some(causal), None) => {
            check_replacements(crdt, &block_states);
            check_causal(crdt, causal, field, inputs);
        }
        (None, Some(tagged)) => {
            check_merges(crdt, &block_states);
            check_tagged(tagged, field, inputs);
        }
        (None, None) => check_merges(crdt, &block_states),
    }
}

/// Check a tagged type on histories of `values`, each the value a write
/// sets; nil values are skipped. For a first write `base`, and writes `a` and
/// `b` made concurrently on top of it, then a write on their merge:
///
/// - a first write's state is the state of its value, and the replay of its
///   block;
/// - a write prepared against a state gives the value it sets, and its
///   operand landed is the replay of the history with its block: a local
///   write has every recorded operation in its past, as a block whose
///   parents are the tips;
/// - an operand round trips through [`FieldOp::Operand`];
/// - concurrent operands land in either order to the same state, and fold
///   into one before their base exactly;
/// - the history replays to the same state in any order, equal to its
///   operands folded one by one, and its value is the value the operands
///   land on;
/// - every tag a replayed state records is the ID of one of its blocks.
///
/// Panics on the first law that fails.
pub fn check_tagged(tagged: &dyn TaggedCrdt, field: &FieldDescription, values: &[NormalValue]) {
    assert!(
        tagged.causal().is_none(),
        "a type is at most one of causal and tagged"
    );
    let write = |state: Option<&StoredField>, value: &NormalValue, priority, parents: &[u8]| {
        let parents: Vec<Vec<u8>> = parents.iter().map(|&parent| vec![parent]).collect();
        local_write(
            tagged,
            field,
            state,
            &FieldOp::Set(value.clone()),
            priority,
            &parents,
            0,
        )
        .expect("a local write prepares")
    };
    let block = |cid: u8, parents: &[u8], priority, write: &LocalWrite| TaggedBlock {
        cid: vec![cid],
        parents: parents.iter().map(|&parent| vec![parent]).collect(),
        priority,
        data: write.data.clone(),
    };
    let landed = |state: Option<&StoredField>, write: &LocalWrite| {
        land(tagged, state, write.operand.clone()).expect("an operand lands")
    };
    let replay = |history: &[TaggedBlock]| {
        let state = tagged
            .replay(field, history)
            .expect("history replays")
            .expect("a history of blocks has a state");
        assert_eq!(
            fold_history(tagged, field, history)
                .expect("history folds")
                .as_ref(),
            Some(&state),
            "the replay of {history:?} against its operands folded"
        );
        let ids: Vec<Vec<u8>> = history
            .iter()
            .map(|block| tagged.op_id(block.priority, &block.parents, &block.data))
            .collect();
        for tag in tagged.tags(field, state.part()).expect("tags read") {
            assert!(
                ids.contains(&tag.id),
                "{tag:?} names a block of the history"
            );
        }
        state
    };
    let value = |state: &StoredField| tagged.materialize(state.part()).expect("state reads");
    let values: Vec<&NormalValue> = values.iter().filter(|value| !value.is_nil()).collect();
    for &base in &values {
        let first = write(None, base, 1, &[]);
        assert_eq!(&first.value, base, "first write {base:?}");
        let root = landed(None, &first);
        assert_eq!(
            value(&root),
            value(&tagged.state(field, 1, base).expect("stored value")),
            "first write {base:?} against its state"
        );
        let first_block = block(0, &[], 1, &first);
        assert_eq!(
            replay(std::slice::from_ref(&first_block)),
            root,
            "first write {base:?} replayed"
        );
        for &a in &values {
            let left = write(Some(&root), a, 2, &[0]);
            assert_eq!(&left.value, a, "{a:?} written on {base:?}");
            let operand = encode_operand(&left.operand).expect("operand encodes");
            assert_eq!(
                decode_operand(&operand).as_ref(),
                Some(&left.operand),
                "operand of {a:?} round trips"
            );
            let left_state = landed(Some(&root), &left);
            let left_block = block(1, &[0], 2, &left);
            assert_eq!(
                replay(&[first_block.clone(), left_block.clone()]),
                left_state,
                "{a:?} on {base:?}: the local write against the replay"
            );
            for &b in &values {
                let right = write(Some(&root), b, 2, &[0]);
                let both = landed(Some(&left_state), &right);
                assert_eq!(
                    both,
                    landed(Some(&landed(Some(&root), &right)), &left),
                    "{a:?} and {b:?} land in either order"
                );
                let mut pair = StoredDocument::default();
                pair.fields.insert(0, left.operand.clone());
                let mut later = StoredDocument::default();
                later.fields.insert(0, right.operand.clone());
                pair.fold(later).expect("operands fold");
                let mut folded = StoredDocument::default();
                folded.fields.insert(0, root.clone());
                folded.apply(pair).expect("a folded operand lands");
                assert_eq!(folded.fields[&0], both, "{a:?} and {b:?} folded first");
                let history = [
                    first_block.clone(),
                    left_block.clone(),
                    block(2, &[0], 2, &right),
                ];
                let merged = replay(&history);
                for order in permutations(&history) {
                    assert_eq!(replay(&order), merged, "merge order {order:?}");
                }
                assert_eq!(
                    value(&merged),
                    value(&both),
                    "{a:?} and {b:?}: the replay against the operands"
                );
                let next = write(Some(&merged), base, 3, &[1, 2]);
                assert_eq!(&next.value, base, "{base:?} written on the merge");
                let mut after = history.to_vec();
                after.push(block(3, &[1, 2], 3, &next));
                assert_eq!(
                    value(&replay(&after)),
                    *base,
                    "a write on the merge of {a:?} and {b:?} replayed"
                );
            }
        }
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
        let data = crdt
            .block_data(value, Some(&op))
            .expect("block data encodes")
            .expect("an edit makes a block");
        causal
            .check_version(field, &data, value)
            .expect("a block fits its own version");
        op
    };
    let values: Vec<&NormalValue> = values.iter().filter(|value| !value.is_nil()).collect();
    for &base in &values {
        let first_data = crdt
            .block_data(base, None)
            .expect("block data encodes")
            .expect("a write of a value makes a block");
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
                let left_value = crdt.apply(field, Some(base), &left.op).expect("applies");
                if let Some(from_first) = causal
                    .merge_from(
                        field,
                        Some(&left_value),
                        &first_data,
                        &concurrent[1..],
                        &concurrent[2].id,
                    )
                    .expect("a merge from the first block")
                {
                    assert_eq!(
                        from_first, merged,
                        "{a:?} and {b:?} merged from the first block"
                    );
                }
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
