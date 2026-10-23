//! Counter CRDT implementation
//!
//! Supports increment and decrement operations. Uses commutative addition with
//! wrapping on overflow for Int64 (matching Go DefraDB), and IEEE-754 addition
//! for Float64. This is not a traditional PN-Counter (which uses separate
//! per-replica counters); instead it uses a single accumulated value.
//!
//! The value is one part of a document value: an increment is a blind merge of
//! a document operand that carries only that part, which the engine adds to
//! the stored value at read and folds in compaction (exactly, for Int64). No
//! read precedes it, so increments of one counter never conflict.
//!
//! Merge is unconditional: idempotency is enforced upstream by the applied
//! keys and the blockstore (`is_merged(cid)` / `get_unmerged()`) on every
//! ingest path. Matches Go's counter Merge which also ignores the delta nonce.

use crate::traits::{Context, Delta, MergeResult, ReplicatedData, ValueReader};
use async_trait::async_trait;
use bytes::Bytes;
use defra_core::{Error, Result};
use document::stored::{Part, StoredDocument, ValueKind};
use document::NormalValue;
use serde::{Deserialize, Serialize};
use std::any::Any;
use storage::{Reader, ReaderWriter};

/// Numeric kind for counter values.
///
/// # Convergence warning (Float32/Float64)
///
/// Counter merge is unconditional addition. For `Int64` this is order-independent
/// (wrapping addition is associative — proven in `proofs/lean` as `word64Add_assoc`).
/// For `Float32`/`Float64` it is **NOT**: IEEE-754 addition is not associative
/// (`(0.1 + 0.2) + 0.3 != 0.1 + (0.2 + 0.3)`; proven as `float_add_not_assoc`), so two
/// replicas applying the same float deltas in different merge orders can converge to
/// **different values**. This matches Go DefraDB exactly (both use raw IEEE-754 `+`), so
/// the behavior is retained for parity — but float counter fields are not convergence-safe.
/// Prefer `Int64` (or a fixed-point/scaled integer) when cross-replica convergence matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NumericKind {
    Int64,
    /// Non-convergent across replicas — see the type-level convergence warning above.
    Float32,
    /// Non-convergent across replicas — see the type-level convergence warning above.
    Float64,
}

impl NumericKind {
    /// How a document value stores a counter of this kind.
    pub fn value_kind(self) -> ValueKind {
        match self {
            NumericKind::Int64 => ValueKind::Int64,
            NumericKind::Float32 => ValueKind::Float32,
            NumericKind::Float64 => ValueKind::Float64,
        }
    }
}

/// Counter Delta - represents an increment/decrement operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterDelta {
    /// Document ID this delta applies to
    doc_id: Vec<u8>,
    /// Field name within the document
    field_name: String,
    /// Priority for DAG ordering (counters merge commutatively, no conflict resolution needed)
    priority: u64,
    /// Nonce ensures idempotent delivery (same delta only applied once)
    nonce: i64,
    /// Schema version identifier
    collection_version_id: String,
    /// The increment/decrement value as big-endian bytes.
    /// Int64/Float64: 8 bytes, Float32: 4 bytes.
    data: Vec<u8>,
    /// Numeric kind (Int64, Float32, or Float64)
    kind: NumericKind,
}

impl CounterDelta {
    /// Create a new Int64 counter delta
    pub fn new_int64(
        doc_id: Vec<u8>,
        field_name: String,
        priority: u64,
        nonce: i64,
        collection_version_id: String,
        increment: i64,
    ) -> Result<Self> {
        if doc_id.is_empty() {
            return Err(Error::MergeError("doc_id cannot be empty".into()));
        }
        if field_name.is_empty() {
            return Err(Error::MergeError("field_name cannot be empty".into()));
        }
        if collection_version_id.is_empty() {
            return Err(Error::MergeError(
                "collection_version_id cannot be empty".into(),
            ));
        }
        Ok(Self {
            doc_id,
            field_name,
            priority,
            nonce,
            collection_version_id,
            data: increment.to_be_bytes().to_vec(),
            kind: NumericKind::Int64,
        })
    }

    /// Create a new Float64 counter delta
    pub fn new_float64(
        doc_id: Vec<u8>,
        field_name: String,
        priority: u64,
        nonce: i64,
        collection_version_id: String,
        increment: f64,
    ) -> Result<Self> {
        if doc_id.is_empty() {
            return Err(Error::MergeError("doc_id cannot be empty".into()));
        }
        if field_name.is_empty() {
            return Err(Error::MergeError("field_name cannot be empty".into()));
        }
        if collection_version_id.is_empty() {
            return Err(Error::MergeError(
                "collection_version_id cannot be empty".into(),
            ));
        }
        Ok(Self {
            doc_id,
            field_name,
            priority,
            nonce,
            collection_version_id,
            data: increment.to_be_bytes().to_vec(),
            kind: NumericKind::Float64,
        })
    }

    /// Create a new Float32 counter delta.
    ///
    /// Matches Go's `FieldKind_NILLABLE_FLOAT32` path. Accumulates with
    /// f32 precision to produce identical results to Go's `validateAndIncrement[float32]`.
    pub fn new_float32(
        doc_id: Vec<u8>,
        field_name: String,
        priority: u64,
        nonce: i64,
        collection_version_id: String,
        increment: f32,
    ) -> Result<Self> {
        if doc_id.is_empty() {
            return Err(Error::MergeError("doc_id cannot be empty".into()));
        }
        if field_name.is_empty() {
            return Err(Error::MergeError("field_name cannot be empty".into()));
        }
        if collection_version_id.is_empty() {
            return Err(Error::MergeError(
                "collection_version_id cannot be empty".into(),
            ));
        }
        Ok(Self {
            doc_id,
            field_name,
            priority,
            nonce,
            collection_version_id,
            data: increment.to_be_bytes().to_vec(),
            kind: NumericKind::Float32,
        })
    }

    /// Get the document ID
    pub fn doc_id(&self) -> &[u8] {
        &self.doc_id
    }

    /// Get the field name
    pub fn field_name(&self) -> &str {
        &self.field_name
    }

    /// Get the priority
    pub fn priority(&self) -> u64 {
        self.priority
    }

    /// Get the nonce
    pub fn nonce(&self) -> i64 {
        self.nonce
    }

    /// Get the collection version ID
    pub fn collection_version_id(&self) -> &str {
        &self.collection_version_id
    }

    /// Get the raw data bytes
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get the numeric kind
    pub fn kind(&self) -> NumericKind {
        self.kind
    }

    /// The increment as a value, once it is checked against a counter of
    /// `kind` that accepts a negative increment only with `allow_decrement`.
    pub fn checked_value(&self, kind: NumericKind, allow_decrement: bool) -> Result<NormalValue> {
        if self.kind != kind {
            return Err(Error::MergeError(format!(
                "numeric kind mismatch for field '{}': counter is {:?}, delta is {:?}",
                self.field_name, kind, self.kind
            )));
        }
        let (negative, value) = match kind {
            NumericKind::Int64 => {
                let value = self.decode_int64()?;
                (value < 0, NormalValue::Int(value))
            }
            NumericKind::Float32 => {
                let value = self.decode_float32()?;
                (value < 0.0, NormalValue::Float32(value))
            }
            NumericKind::Float64 => {
                let value = self.decode_float64()?;
                (value < 0.0, NormalValue::Float64(value))
            }
        };
        if negative && !allow_decrement {
            return Err(Error::MergeError("decrement not allowed".into()));
        }
        Ok(value)
    }

    /// Decode the increment value as i64
    pub fn decode_int64(&self) -> Result<i64> {
        let arr: [u8; 8] = self.data[..].try_into().map_err(|_| {
            Error::MergeError(format!(
                "invalid Int64 data length: expected 8, got {}",
                self.data.len()
            ))
        })?;
        Ok(i64::from_be_bytes(arr))
    }

    /// Decode the increment value as f32
    pub fn decode_float32(&self) -> Result<f32> {
        let arr: [u8; 4] = self.data[..].try_into().map_err(|_| {
            Error::MergeError(format!(
                "invalid Float32 data length: expected 4, got {}",
                self.data.len()
            ))
        })?;
        Ok(f32::from_be_bytes(arr))
    }

    /// Decode the increment value as f64
    pub fn decode_float64(&self) -> Result<f64> {
        let arr: [u8; 8] = self.data[..].try_into().map_err(|_| {
            Error::MergeError(format!(
                "invalid Float64 data length: expected 8, got {}",
                self.data.len()
            ))
        })?;
        Ok(f64::from_be_bytes(arr))
    }
}

impl Delta for CounterDelta {
    fn get_priority(&self) -> u64 {
        self.priority
    }

    fn set_priority(&mut self, priority: u64) {
        self.priority = priority;
    }

    fn doc_id(&self) -> &[u8] {
        &self.doc_id
    }

    fn field_name(&self) -> &str {
        &self.field_name
    }

    fn collection_version_id(&self) -> &str {
        &self.collection_version_id
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Counter CRDT - supports increment/decrement with commutative merge
///
/// This CRDT does not own storage. It receives a `ReaderWriter` for each
/// operation, matching Go DefraDB's pattern where CRDTs operate on a provided
/// `corekv.ReaderWriter`, and keeps its value in part `part` of the document
/// value stored at `value_key`.
pub struct Counter {
    /// Key of the document value holding the counter
    value_key: Vec<u8>,
    /// The counter's part ID in that document value
    part: u32,
    /// Schema version
    collection_version_id: String,
    /// Field name
    field_name: String,
    /// Whether decrement is allowed
    allow_decrement: bool,
    /// Numeric kind (int or float)
    kind: NumericKind,
}

impl Counter {
    /// Create a new Counter CRDT
    ///
    /// # Arguments
    /// * `collection_version_id` - Schema version identifier (must not be empty)
    /// * `value_key` - Key of the document value (must not be empty)
    /// * `part` - The field's part ID in that value
    /// * `field_name` - Field name (must not be empty)
    /// * `allow_decrement` - Whether negative increments are allowed
    /// * `kind` - Numeric type (Int64, Float32 or Float64)
    ///
    /// # Errors
    /// Returns an error if collection_version_id, value_key, or field_name is empty.
    pub fn new(
        collection_version_id: String,
        value_key: Vec<u8>,
        part: u32,
        field_name: String,
        allow_decrement: bool,
        kind: NumericKind,
    ) -> Result<Self> {
        if collection_version_id.is_empty() {
            return Err(Error::MergeError(
                "collection_version_id cannot be empty".into(),
            ));
        }
        if value_key.is_empty() {
            return Err(Error::MergeError("value_key cannot be empty".into()));
        }
        if field_name.is_empty() {
            return Err(Error::MergeError("field_name cannot be empty".into()));
        }
        Ok(Self {
            value_key,
            part,
            collection_version_id,
            field_name,
            allow_decrement,
            kind,
        })
    }

    /// The document operand that adds `delta`, after checking it is a delta
    /// this counter accepts.
    fn operand(&self, delta: &CounterDelta) -> Result<Vec<u8>> {
        delta.checked_value(self.kind, self.allow_decrement)?;
        // A checked delta's bytes are its increment in the counter's own
        // big-endian width, exactly what the part stores.
        storage::merge::document_part(
            self.part,
            Part {
                name: &self.field_name,
                kind: self.kind.value_kind(),
                priority: delta.priority(),
                value: delta.data(),
            },
        )
        .ok_or_else(|| {
            Error::MergeError(format!("invalid counter delta for '{}'", self.field_name))
        })
    }

    /// Apply an increment/decrement as a blind merge of its operand.
    ///
    /// Merge is unconditional: every delta that reaches this method is
    /// applied. Per-delta idempotency is the caller's applied key and the
    /// blockstore's `is_merged(cid)` / `get_unmerged()` on every ingest path
    /// (PushLog, DAG traversal, crash recovery). Do not reintroduce a
    /// nonce-based dedup check here: doing so causes Rust/Go state divergence
    /// on legitimate block retransmits (#847). The `delta.nonce` field exists
    /// only so the resulting DAG block has a unique CID.
    async fn apply_delta(
        &self,
        rw: &mut dyn ReaderWriter,
        delta: &CounterDelta,
    ) -> Result<MergeResult> {
        let operand = self.operand(delta)?;
        rw.merge(&self.value_key, &operand)
            .await
            .map_err(|error| Error::Storage(error.to_string()))?;
        Ok(MergeResult::Applied)
    }

    /// Current value bytes. The read is never validated: no decision rests on
    /// it, and an increment must not turn into a read-modify-write.
    async fn get_value_internal(&self, reader: &dyn Reader) -> Result<Bytes> {
        let missing = || {
            Error::MergeError(format!(
                "counter value not found for field '{}' in schema '{}'",
                self.field_name, self.collection_version_id
            ))
        };
        let bytes = reader
            .get_parts(&self.value_key, &[])
            .await
            .map_err(|e| Error::Storage(e.to_string()))?
            .ok_or_else(missing)?;
        let stored = StoredDocument::decode(&bytes).ok_or_else(missing)?;
        let field = stored.fields.get(&self.part).ok_or_else(missing)?;
        Ok(Bytes::copy_from_slice(&field.value))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ReplicatedData for Counter {
    async fn merge(
        &self,
        rw: &mut dyn ReaderWriter,
        _ctx: &Context,
        delta: &dyn Delta,
    ) -> Result<MergeResult> {
        // Downcast to CounterDelta
        let counter_delta = delta
            .as_any()
            .downcast_ref::<CounterDelta>()
            .ok_or_else(|| {
                Error::MergeError(
                    "invalid delta type for Counter merge: expected CounterDelta".into(),
                )
            })?;

        // Validate field name
        if counter_delta.field_name != self.field_name {
            return Err(Error::MergeError(format!(
                "field name mismatch: expected {}, got {}",
                self.field_name, counter_delta.field_name
            )));
        }

        // Validate schema version
        if counter_delta.collection_version_id != self.collection_version_id {
            return Err(Error::MergeError(format!(
                "schema version mismatch: expected {}, got {}",
                self.collection_version_id, counter_delta.collection_version_id
            )));
        }

        self.apply_delta(rw, counter_delta).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ValueReader for Counter {
    async fn value(&self, reader: &dyn Reader) -> Result<Bytes> {
        self.get_value_internal(reader).await
    }
}
