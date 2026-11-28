//! One pending write of a deferred vector index: its key and its value, in
//! Go's layout (`internal/db/vectorindex/pending.go`,
//! `proofs/lean/VectorPending`).
//!
//! The key is `/<collection>/<index>/<epoch>/p/<node>`, the node an ascending
//! uvarint, so a scan visits the entries in node order. The value is one op
//! byte, then the vector as little-endian `f32`s.

use storage::encoding::{decode_uvarint_ascending, encode_uvarint_ascending};

use crate::index::error::{Error, Result};
use crate::index::vector::store::NodeId;

/// The aux kind of pending entries, beside the graph's `m` and `n`.
pub const PENDING: u8 = b'p';

/// What a pending entry does to its node. Stored as the value's first byte,
/// so the numbers never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOp {
    /// Sets the node's vector, replacing any the graph holds.
    Upsert = 1,
    /// Removes the node from the results.
    Delete = 2,
}

/// A write not yet applied to the graph. A node has at most one, so a later
/// write to it replaces an earlier one.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingEntry {
    pub id: NodeId,
    pub op: PendingOp,
    /// The prepared vector; empty for a delete.
    pub vector: Vec<f32>,
}

impl PendingEntry {
    pub fn encode(&self) -> Vec<u8> {
        encode_value(self.op, &self.vector)
    }
}

/// The op byte, then each element as four little-endian bytes.
pub fn encode_value(op: PendingOp, vector: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 4 * vector.len());
    out.push(op as u8);
    for element in vector {
        out.extend_from_slice(&element.to_le_bytes());
    }
    out
}

/// Refuses a value with no op byte, a partial element, or an unknown op.
pub fn decode_value(id: NodeId, bytes: &[u8]) -> Result<PendingEntry> {
    let malformed = || Error::MalformedVectorPending {
        node: id.0,
        length: bytes.len(),
    };
    let (&op, rest) = bytes.split_first().ok_or_else(malformed)?;
    if rest.len() % 4 != 0 {
        return Err(malformed());
    }
    let op = match op {
        1 => PendingOp::Upsert,
        2 => PendingOp::Delete,
        op => return Err(Error::UnknownVectorPendingOp { node: id.0, op }),
    };
    let vector = rest
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect();
    Ok(PendingEntry { id, op, vector })
}

/// The node part of a pending key, after the kind's separator.
pub fn node_key(id: NodeId) -> Vec<u8> {
    encode_uvarint_ascending(Vec::with_capacity(9), id.0)
}

/// The node a pending key names; anything after the node is refused.
pub fn node_from_key(key: &[u8]) -> Result<NodeId> {
    match decode_uvarint_ascending(key) {
        Ok(([], id)) if id != 0 => Ok(NodeId(id)),
        _ => Err(Error::Other(format!(
            "invalid vector pending key {}",
            hex::encode(key)
        ))),
    }
}
