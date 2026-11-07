//! CRDT delta payload types embedded in blocks.
//!
//! These match the Go `crdt.*Delta` structures for wire compatibility.

use std::collections::BTreeMap;

use cid::Cid;
use serde::{Deserialize, Serialize};

use crate::block_signature::DocumentStatus;

/// LWW Register delta payload for block embedding
///
/// Matches Go's `crdt.LWWDelta` structure. Deltas carry no document
/// identity: the public DocID is derived from the genesis composite
/// block CID (Go #4838).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LwwDeltaPayload {
    /// Field name
    #[serde(rename = "fieldName")]
    pub field_name: String,

    /// Priority for conflict resolution
    pub priority: u64,

    /// Collection version identifier
    #[serde(rename = "collectionVersionID")]
    pub collection_version_id: String,

    /// The value data (empty = deletion/tombstone)
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

/// Composite link name of a document's Immutable block. Reserved: schema
/// validation rejects a field with this name.
pub const IMMUTABLE_LINK_NAME: &str = "_immutable";

/// Immutable delta payload: every `@immutable` field of a document, written
/// once at create in a single block.
///
/// An immutable field is set once, so its first value is final and there is
/// nothing to merge. One block holding all of them replaces one LWW block per
/// field. A Rust-only delta: Go peers cannot decode it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmutableDeltaPayload {
    /// Priority (always 1: written by the genesis composite)
    pub priority: u64,

    /// Collection version identifier
    #[serde(rename = "collectionVersionID")]
    pub collection_version_id: String,

    /// Field name to CBOR-encoded value, as an LWW delta's `data`
    pub fields: BTreeMap<String, serde_bytes::ByteBuf>,
}

impl ImmutableDeltaPayload {
    /// The fields as the LWW deltas they replace, in field-name order.
    pub fn lww_deltas(&self) -> impl Iterator<Item = LwwDeltaPayload> + '_ {
        self.fields
            .iter()
            .map(|(field_name, data)| LwwDeltaPayload {
                field_name: field_name.clone(),
                priority: self.priority,
                collection_version_id: self.collection_version_id.clone(),
                data: data.to_vec(),
            })
    }
}

/// Text delta payload for block embedding: edits to a Text field.
///
/// Shaped as an LWW delta, so a field's encryption treats both alike; `data`
/// is the CBOR of the block's [`TextOp`](crate::text_op::TextOp)s. The ops
/// count positions in the block's parent version, the text its `heads`
/// produce. A Rust-only delta: Go peers cannot decode it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextDeltaPayload {
    /// Field name
    #[serde(rename = "fieldName")]
    pub field_name: String,

    /// Priority
    pub priority: u64,

    /// Collection version identifier
    #[serde(rename = "collectionVersionID")]
    pub collection_version_id: String,

    /// The encoded ops
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

/// JSON delta payload for block embedding: edits at paths of a JSON field.
///
/// Shaped as an LWW delta, so a field's encryption treats both alike; `data`
/// is the CBOR of the block's [`JsonOp`](crate::json_crdt::JsonOp)s, one per
/// path. A Rust-only delta: Go peers cannot decode it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonDeltaPayload {
    /// Field name
    #[serde(rename = "fieldName")]
    pub field_name: String,

    /// Priority
    pub priority: u64,

    /// Collection version identifier
    #[serde(rename = "collectionVersionID")]
    pub collection_version_id: String,

    /// The encoded ops
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

/// Custom delta payload for block embedding: a field block written by a CRDT
/// type registered on the node by name.
///
/// Shaped as an LWW delta, so a field's encryption treats it alike; `data` is
/// whatever the type encodes. `crdt` names the type, so a node that does not
/// have it can refuse the block by name. `nonce` keeps two equal operations
/// distinct blocks, for a type that replicates operations; a state type leaves
/// it out. A Rust-only delta: Go peers cannot decode it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomDeltaPayload {
    /// The custom CRDT type's name
    pub crdt: String,

    /// Field name
    #[serde(rename = "fieldName")]
    pub field_name: String,

    /// Priority
    pub priority: u64,

    /// Nonce for idempotency, for a type that replicates operations
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<i64>,

    /// Collection version identifier
    #[serde(rename = "collectionVersionID")]
    pub collection_version_id: String,

    /// The encoded value or operation
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

/// Counter delta payload for block embedding
///
/// Matches Go's `crdt.CounterDelta` structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterDeltaPayload {
    /// Field name
    #[serde(rename = "fieldName")]
    pub field_name: String,

    /// Priority
    pub priority: u64,

    /// Nonce for idempotency
    pub nonce: i64,

    /// Collection version identifier
    #[serde(rename = "collectionVersionID")]
    pub collection_version_id: String,

    /// Increment/decrement value (encoded)
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

/// Composite delta payload for block embedding
///
/// Matches Go's `crdt.DocCompositeDelta` structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeDeltaPayload {
    /// Collection version identifier
    #[serde(rename = "collectionVersionID")]
    pub collection_version_id: String,

    /// Priority
    pub priority: u64,

    /// Document status (1 = active, 2 = deleted)
    #[serde(deserialize_with = "deserialize_document_status")]
    pub status: u8,
}

impl CompositeDeltaPayload {
    pub(crate) fn validate_status(status: u8) -> Result<u8, String> {
        DocumentStatus::from_u8(status)
            .map(|_| status)
            .ok_or_else(|| format!("invalid document status: {status}"))
    }
}

fn deserialize_document_status<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let status = u8::deserialize(deserializer)?;
    CompositeDeltaPayload::validate_status(status).map_err(serde::de::Error::custom)
}

/// Collection delta payload for block embedding
///
/// Matches Go's `crdt.CollectionDelta` structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionDeltaPayload {
    /// Collection version identifier
    #[serde(rename = "collectionVersionID")]
    pub collection_version_id: String,

    /// Priority
    pub priority: u64,
}

/// Field definition delta payload for schema versioning
///
/// Matches Go's `crdt.FieldDefinitionDelta` structure.
/// Used to track changes to field definitions across schema versions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDefinitionDeltaPayload {
    /// Priority
    pub priority: u64,

    /// Field name (optional for updates)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// CRDT type for this field
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crdt: Option<u8>,

    /// The custom CRDT type's name, beside `crdt`'s custom code. Skipped when
    /// absent, so a native field's delta serialises to the bytes it did
    /// before custom types existed.
    #[serde(
        rename = "customCrdt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub custom_crdt: Option<String>,

    /// Scalar kind (for scalar fields)
    #[serde(
        rename = "scalarKind",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub scalar_kind: Option<u8>,

    /// Related collection ID (for relation fields)
    #[serde(
        rename = "collectionID",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub collection_id: Option<String>,

    /// Relative ID (for self-referencing fields)
    #[serde(
        rename = "relativeID",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub relative_id: Option<i32>,

    /// Whether the field is set once and never changed.
    ///
    /// A commitment to writers, not a local choice, so it belongs in the
    /// field's identity. Skipped when false, so a mutable field's delta
    /// serialises to the bytes it did before this field existed.
    #[serde(rename = "immutable", default, skip_serializing_if = "is_false")]
    pub immutable: bool,
}

/// `skip_serializing_if` for a flag whose absence means false.
fn is_false(value: &bool) -> bool {
    !*value
}

impl FieldDefinitionDeltaPayload {
    /// Create a new field definition delta
    pub fn new(priority: u64) -> Self {
        Self {
            priority,
            name: None,
            crdt: None,
            custom_crdt: None,
            scalar_kind: None,
            collection_id: None,
            relative_id: None,
            immutable: false,
        }
    }

    /// Set the field name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the CRDT type
    pub fn with_crdt(mut self, crdt: u8) -> Self {
        self.crdt = Some(crdt);
        self
    }

    /// Set the custom CRDT type's name
    pub fn with_custom_crdt(mut self, name: impl Into<String>) -> Self {
        self.custom_crdt = Some(name.into());
        self
    }

    /// Set the scalar kind
    pub fn with_scalar_kind(mut self, kind: u8) -> Self {
        self.scalar_kind = Some(kind);
        self
    }

    /// Set the related collection ID
    pub fn with_collection_id(mut self, id: impl Into<String>) -> Self {
        self.collection_id = Some(id.into());
        self
    }

    /// Set the relative ID
    pub fn with_relative_id(mut self, id: i32) -> Self {
        self.relative_id = Some(id);
        self
    }

    /// Mark the field as set once and never changed.
    pub fn with_immutable(mut self, immutable: bool) -> Self {
        self.immutable = immutable;
        self
    }
}

/// Collection definition delta payload for schema versioning
///
/// Matches Go's `crdt.CollectionDefinitionDelta` structure.
/// Used to track changes to collection definitions across schema versions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionDefinitionDeltaPayload {
    /// Priority
    pub priority: u64,

    /// Collection name (optional for updates)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Query select for view collections (JSON-encoded query definition)
    #[serde(
        rename = "querySelect",
        default,
        skip_serializing_if = "Option::is_none",
        with = "serde_bytes"
    )]
    pub query_select: Option<Vec<u8>>,

    /// Query transform CID for view collections (link to lens transform)
    #[serde(
        rename = "queryTransform",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub query_transform: Option<Cid>,

    /// The governance root claiming this collection, when one does.
    ///
    /// Skipped when absent, so an ungoverned collection's delta serialises to
    /// exactly the bytes it did before this field existed and keeps its CID.
    #[serde(
        rename = "governance",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub governance_root: Option<String>,

    /// Whether the collection's history is tracked as one verifiable entity.
    ///
    /// Changes what a writer is promised about the collection's history, so
    /// it belongs in the identity. Skipped when false.
    #[serde(rename = "branchable", default, skip_serializing_if = "is_false")]
    pub is_branchable: bool,

    /// The collection's policy, as a CID over its reference.
    ///
    /// Present in the delta the version ID is taken over and absent from the
    /// one the collection ID is taken over, so a policy change mints a new
    /// version of the same collection.
    #[serde(rename = "policy", default, skip_serializing_if = "Option::is_none")]
    pub policy_cid: Option<Cid>,

    /// The `@chunk` fields' settings. They decide how text is cut, so two
    /// nodes must not hold one version with different settings.
    #[serde(rename = "chunked", default, skip_serializing_if = "Vec::is_empty")]
    pub chunked_fields: Vec<ChunkedFieldDelta>,

    /// The parent field a generated chunk collection holds the chunks of.
    #[serde(rename = "chunkOf", default, skip_serializing_if = "Option::is_none")]
    pub chunk_of: Option<Box<ChunkOfDelta>>,
}

/// One `@chunk` field's settings in its collection's delta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkedFieldDelta {
    #[serde(rename = "field")]
    pub field: String,
    #[serde(rename = "maxChars")]
    pub max_chars: u32,
    #[serde(rename = "overlap")]
    pub overlap: u32,
}

/// A generated chunk collection's parent field in its delta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkOfDelta {
    #[serde(rename = "collection")]
    pub collection: String,
    #[serde(rename = "field")]
    pub field: String,
}

impl CollectionDefinitionDeltaPayload {
    /// Create a new collection definition delta
    pub fn new(priority: u64) -> Self {
        Self {
            priority,
            name: None,
            query_select: None,
            query_transform: None,
            governance_root: None,
            is_branchable: false,
            policy_cid: None,
            chunked_fields: Vec::new(),
            chunk_of: None,
        }
    }

    /// Set the collection name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the query select (for view collections)
    pub fn with_query_select(mut self, query: Vec<u8>) -> Self {
        self.query_select = Some(query);
        self
    }

    /// Set the query transform CID (for view collections with lens transforms)
    pub fn with_query_transform(mut self, transform_cid: Cid) -> Self {
        self.query_transform = Some(transform_cid);
        self
    }

    /// Set the governance root claiming this collection.
    pub fn with_governance_root(mut self, root: impl Into<String>) -> Self {
        self.governance_root = Some(root.into());
        self
    }

    /// Mark the collection's history as one verifiable entity.
    pub fn with_branchable(mut self, is_branchable: bool) -> Self {
        self.is_branchable = is_branchable;
        self
    }

    /// Set the CID over the collection's policy reference.
    pub fn with_policy_cid(mut self, policy_cid: Cid) -> Self {
        self.policy_cid = Some(policy_cid);
        self
    }

    /// Set the `@chunk` fields' settings.
    pub fn with_chunked_fields(mut self, chunked_fields: Vec<ChunkedFieldDelta>) -> Self {
        self.chunked_fields = chunked_fields;
        self
    }

    /// Set the parent field of a generated chunk collection.
    pub fn with_chunk_of(mut self, chunk_of: ChunkOfDelta) -> Self {
        self.chunk_of = Some(Box::new(chunk_of));
        self
    }
}

/// Collection set delta payload for schema versioning.
///
/// Matches Go's `crdt.CollectionSetDelta` structure.
/// Used to generate a CollectionSetID CID for circular relation groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionSetDeltaPayload {
    pub priority: u64,
}

impl CollectionSetDeltaPayload {
    pub fn new(priority: u64) -> Self {
        Self { priority }
    }
}
