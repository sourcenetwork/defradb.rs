use document::{FieldOp, NormalValue};
use schema::FieldDescription;

use super::{FieldCrdt, FieldCrdtError};

/// One block of a field's history, as a causal replay reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct CausalBlock {
    /// The block's ID: its CID bytes, the same on every replica.
    pub id: Vec<u8>,
    /// The IDs of the field's blocks it was written on top of. The value they
    /// produce is the block's parent version.
    pub parents: Vec<Vec<u8>>,
    /// The block's operation, counted in its parent version.
    pub op: FieldOp,
}

/// A type whose operations count in the value they edit (positions in a
/// sequence, say), so a block applies only to its parent version: the value
/// its parent blocks produce. Such a type merges by causal replay:
///
/// - a block that is one of the field's heads is already merged;
/// - a block whose parents are exactly the field's heads applies to the stored
///   value through [`FieldCrdt::apply`], since the stored value is then its
///   parent version;
/// - any other block replays the field's history: every block under the
///   field's heads, and the block.
///
/// The stored value is the replay of the field's history. Every write and
/// merge of the field reads it validated and replaces it whole, so its part
/// of the document value replaces in commit order. A write expresses its
/// operation against the stored value it read ([`diff`](Self::diff) for a
/// write that sets the value), and a write with no operation is the field's
/// first, against the type's empty value. The field's heads move only with
/// its document's composite block, so a block of such a type merges only
/// through its composite.
pub trait CausalCrdt: Send + Sync {
    /// The operation that turns `current` into `value`: how a write that sets
    /// the field is expressed against the stored value it replaces.
    fn diff(
        &self,
        current: Option<&NormalValue>,
        value: &NormalValue,
    ) -> Result<FieldOp, FieldCrdtError>;

    /// The operation a block of this type carries in `data`, decrypted.
    fn block_op(&self, field: &FieldDescription, data: &[u8]) -> Result<FieldOp, FieldCrdtError>;

    /// The value a field's history gives. `history` holds each block once,
    /// with every parent its blocks name, in any order: every order gives the
    /// same value. A [`FieldOp::Set`] block restates the value: it replaces
    /// whatever its parent version holds.
    fn replay(
        &self,
        field: &FieldDescription,
        history: &[CausalBlock],
    ) -> Result<NormalValue, FieldCrdtError>;
}

/// The operation a block of a causal field's history makes: its own, when a
/// causal type wrote it, or else a restatement of the value it carries (a
/// block written before the field took its type).
pub fn history_op(
    writer: &dyn FieldCrdt,
    field: &FieldDescription,
    data: &[u8],
) -> Result<FieldOp, FieldCrdtError> {
    match writer.causal() {
        Some(causal) => causal.block_op(field, data),
        None => Ok(FieldOp::Set(writer.decode(field, data)?)),
    }
}
