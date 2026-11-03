//! Tagged types: a state that records, by ID, the operations that made it,
//! and an operation whose effect depends on which of those its writer had
//! seen. An observed-remove set, a multi-value register and the Json CRDT's
//! per-path frontier are all of this kind.
//!
//! A block's operation takes an ID its writer knows before the block exists
//! ([`TaggedCrdt::op_id`]). Its operand on the stored state removes or
//! supersedes only the recorded operations in the block's causal past
//! ([`TaggedCrdt::effect`]), so it never decides on anything else: writers
//! need no validated read, operands compose blind, and concurrent ones
//! commute. A local write has every recorded operation in its past. A remote
//! block learns its past by walking its own ancestors, highest priority
//! first, until every recorded operation is found among them or has a higher
//! priority than any ancestor left to visit.

use std::collections::{BTreeMap, BTreeSet};

use document::{FieldOp, NormalValue};
use schema::FieldDescription;

use super::{FieldCrdt, FieldCrdtError};
use crate::stored::{encode_parts, Part, StoredDocument, StoredField};

/// Whether an operation, by its ID, is in the causal past of the block whose
/// effect is being taken.
pub type InPast<'tags> = dyn Fn(&[u8]) -> bool + 'tags;

/// One operation a tagged state records.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tag {
    /// The priority of the block that made it.
    pub priority: u64,
    /// Its ID ([`TaggedCrdt::op_id`]).
    pub id: Vec<u8>,
}

/// One block of a tagged field's history, as [`TaggedCrdt::replay`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedBlock {
    /// The block's CID bytes.
    pub cid: Vec<u8>,
    /// Its parents' CID bytes.
    pub parents: Vec<Vec<u8>>,
    pub priority: u64,
    /// Its data, decrypted.
    pub data: Vec<u8>,
}

/// A tagged type: its state records the operations that made it by ID, and
/// an operation's effect depends on which of them its writer had seen. A
/// type implements at most one of this and
/// [`CausalCrdt`](super::CausalCrdt).
///
/// The stored state is the operands of its blocks merged in commit order
/// ([`FieldCrdt::merge`]), settled ([`settle`](Self::settle)). An operand
/// keeps what it removes, so operands fold before their base is known.
pub trait TaggedCrdt: FieldCrdt {
    /// The ID of the operation a block at `priority`, written on `parents`
    /// (their CID bytes) and carrying `data` (decrypted), makes. Every replica
    /// computes it from the block, and the writer from the same three before
    /// the block exists, so it never depends on the block's CID.
    fn op_id(&self, priority: u64, parents: &[Vec<u8>], data: &[u8]) -> Vec<u8>;

    /// The operations `state` records. A merge looks for these, and only
    /// these, among the ancestors of the block it merges.
    fn tags(&self, field: &FieldDescription, state: Part<'_>) -> Result<Vec<Tag>, FieldCrdtError>;

    /// The data of the block a local write of `op` on `state`, the stored
    /// state (`None` when the field has none), carries. `nonce` is drawn for
    /// this write: equal writes by two peers on the same heads carry
    /// different data, so their operations take different IDs.
    fn write_data(
        &self,
        field: &FieldDescription,
        state: Option<Part<'_>>,
        op: &FieldOp,
        nonce: i64,
    ) -> Result<Vec<u8>, FieldCrdtError>;

    /// The operand the operation `id` of a block at `priority` carrying
    /// `data` lands on `state`. It touches only the recorded operations
    /// `in_past` names, and adds the block's own.
    fn effect(
        &self,
        field: &FieldDescription,
        state: Option<Part<'_>>,
        data: &[u8],
        id: &[u8],
        priority: u64,
        in_past: &InPast<'_>,
    ) -> Result<StoredField, FieldCrdtError>;

    /// Drop from `state`, now a stored value rather than an operand on one,
    /// what only an operand needs. `None` when `state` is malformed.
    fn settle(&self, state: &mut StoredField) -> Option<()>;

    /// The state of a field's whole `history`, each block once with every
    /// parent its blocks name, in any order; `None` for an empty history. It
    /// equals [`fold_history`], which it is by default; a type replays faster
    /// by reading only what each block touches.
    fn replay(
        &self,
        field: &FieldDescription,
        history: &[TaggedBlock],
    ) -> Result<Option<StoredField>, FieldCrdtError> {
        fold_history(self, field, history)
    }
}

/// `operand` landed on `state`, settled: the stored state after it.
pub fn land<T>(tagged: &T, state: Option<&StoredField>, operand: StoredField) -> Option<StoredField>
where
    T: TaggedCrdt + ?Sized,
{
    let mut landed = match state {
        Some(state) => {
            let mut state = state.clone();
            tagged.merge(&mut state, operand)?;
            state
        }
        None => operand,
    };
    tagged.settle(&mut landed)?;
    Some(landed)
}

/// A local write, prepared against `state`: the block at `priority` on
/// `parents` carries the data, the operand has every recorded operation in
/// its past, and the value is the state after it.
pub struct LocalWrite {
    pub data: Vec<u8>,
    pub operand: StoredField,
    pub value: NormalValue,
}

impl LocalWrite {
    /// This write as a field's pending operation.
    pub fn op(&self) -> Option<FieldOp> {
        Some(FieldOp::Operand {
            data: self.data.clone(),
            operand: encode_operand(&self.operand)?,
        })
    }
}

/// Prepare a local write of `op` on `state` by a block at `priority` on
/// `parents`, with the `nonce` drawn for it ([`TaggedCrdt::write_data`]).
pub fn local_write<T>(
    tagged: &T,
    field: &FieldDescription,
    state: Option<&StoredField>,
    op: &FieldOp,
    priority: u64,
    parents: &[Vec<u8>],
    nonce: i64,
) -> Result<LocalWrite, FieldCrdtError>
where
    T: TaggedCrdt + ?Sized,
{
    let malformed = || FieldCrdtError::Decode {
        field: field.name.clone(),
        message: "the stored state does not merge".into(),
    };
    let part = state.map(StoredField::part);
    let data = tagged.write_data(field, part, op, nonce)?;
    let id = tagged.op_id(priority, parents, &data);
    let operand = tagged.effect(field, part, &data, &id, priority, &|_| true)?;
    let landed = land(tagged, state, operand.clone()).ok_or_else(malformed)?;
    let value = tagged.materialize(landed.part()).ok_or_else(malformed)?;
    Ok(LocalWrite {
        data,
        operand,
        value,
    })
}

/// `operand` as [`FieldOp::Operand`] carries it: a one-part document value.
pub fn encode_operand(operand: &StoredField) -> Option<Vec<u8>> {
    encode_parts(&[], std::iter::once((0, operand.part())))
}

/// The operand [`FieldOp::Operand`] carries.
pub fn decode_operand(bytes: &[u8]) -> Option<StoredField> {
    StoredDocument::decode(bytes)?.fields.into_values().next()
}

/// A block a replay has merged: its operation's ID, priority and parents.
struct Merged<'block> {
    id: Vec<u8>,
    priority: u64,
    parents: &'block [Vec<u8>],
}

/// The state of a tagged field's whole history, as its blocks' operands
/// landed one by one: every block merged after its parents, which a higher
/// priority than each of them guarantees. A block whose parents are the tips
/// merged so far has every recorded operation in its past; any other walks
/// down its ancestors only as far as the lowest priority recorded. `None` for
/// an empty history.
pub fn fold_history<T>(
    tagged: &T,
    field: &FieldDescription,
    history: &[TaggedBlock],
) -> Result<Option<StoredField>, FieldCrdtError>
where
    T: TaggedCrdt + ?Sized,
{
    let malformed = || FieldCrdtError::History {
        field: field.name.clone(),
        message: "a block's operand does not merge".into(),
    };
    let mut blocks: Vec<&TaggedBlock> = history.iter().collect();
    blocks.sort_by(|a, b| (a.priority, &a.cid).cmp(&(b.priority, &b.cid)));
    let mut merged: BTreeMap<&[u8], Merged<'_>> = BTreeMap::new();
    let mut tips: BTreeSet<&[u8]> = BTreeSet::new();
    let mut state: Option<StoredField> = None;
    for block in blocks {
        let id = tagged.op_id(block.priority, &block.parents, &block.data);
        let parents: BTreeSet<&[u8]> = block.parents.iter().map(Vec::as_slice).collect();
        let all_past = parents == tips;
        let past = match &state {
            Some(state) if !all_past => {
                let lowest = tagged
                    .tags(field, state.part())?
                    .into_iter()
                    .map(|tag| tag.priority)
                    .min();
                past_ids(&merged, &block.parents, lowest.unwrap_or(u64::MAX))
            }
            _ => BTreeSet::new(),
        };
        let in_past = |tag: &[u8]| all_past || past.contains(tag);
        let operand = tagged.effect(
            field,
            state.as_ref().map(StoredField::part),
            &block.data,
            &id,
            block.priority,
            &in_past,
        )?;
        state = Some(land(tagged, state.as_ref(), operand).ok_or_else(malformed)?);
        for parent in &parents {
            tips.remove(parent);
        }
        tips.insert(&block.cid);
        merged.insert(
            &block.cid,
            Merged {
                id,
                priority: block.priority,
                parents: &block.parents,
            },
        );
    }
    Ok(state)
}

/// The operation IDs of the merged blocks below `parents`, down to priority
/// `lowest`: no block further down can have made a recorded operation.
fn past_ids(
    merged: &BTreeMap<&[u8], Merged<'_>>,
    parents: &[Vec<u8>],
    lowest: u64,
) -> BTreeSet<Vec<u8>> {
    let mut ids = BTreeSet::new();
    let mut visited: BTreeSet<&[u8]> = BTreeSet::new();
    let mut waiting: BTreeSet<(u64, &[u8])> = BTreeSet::new();
    let mut next: Vec<&[u8]> = parents.iter().map(Vec::as_slice).collect();
    loop {
        for cid in next.drain(..) {
            if let Some((cid, block)) = merged.get_key_value(cid) {
                if visited.insert(cid) {
                    waiting.insert((block.priority, cid));
                }
            }
        }
        let Some((priority, cid)) = waiting.pop_last() else {
            break;
        };
        if priority < lowest {
            break;
        }
        if let Some(block) = merged.get(cid) {
            ids.insert(block.id.clone());
            next.extend(block.parents.iter().map(Vec::as_slice));
        }
    }
    ids
}
