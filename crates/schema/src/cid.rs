//! CID (Content Identifier) generation for schema definitions.
//!
//! This module generates CIDs compatible with Go DefraDB's IPLD block format.
//! Uses defra-core's Block type with serde_ipld_dagcbor for proper DAG-CBOR encoding.

use cid::Cid;
use defra_core::{
    Block, CollectionDefinitionDeltaPayload, CollectionSetDeltaPayload, CrdtDelta, DAGLink,
    FieldDefinitionDeltaPayload, DAG_CBOR_CODEC, SHA2_256_CODE,
};
use multihash::Multihash;
use sha2::{Digest, Sha256};

use crate::{FieldDescription, FieldKind};

/// Block with its CID and serialized DAG-CBOR bytes.
///
/// Used when blocks need to be stored in the blockstore, not just have their CID computed.
#[derive(Debug, Clone)]
pub struct BlockWithCid {
    /// The content identifier for this block.
    pub cid: Cid,
    /// The DAG-CBOR serialized bytes of the block.
    pub bytes: Vec<u8>,
}

/// What a collection's identity commits to beyond the shape of its data.
///
/// These are promises to writers rather than choices a node makes for itself,
/// so they belong in the identity: a collection under a different root, or one
/// whose history is verifiable when another's is not, is a different
/// collection.
///
/// A schema opts into carrying them by declaring a governance root. Without
/// one, nothing here reaches the delta and every identity is byte-identical to
/// what it was before any of this existed, whatever `@immutable` or
/// `@branchable` the schema carries. The gate is the root because identity is
/// derived from the schema: an opt-in that lived in node configuration would
/// let two nodes running the same schema disagree about what a collection is,
/// which is the failure the commitment exists to rule out.
#[derive(Debug, Clone, Copy, Default)]
pub struct Commitments<'a> {
    /// The governance root claiming the collection, from `@governed`.
    pub governance_root: Option<&'a str>,
    /// Whether the collection's history is one verifiable entity.
    pub is_branchable: bool,
    /// The collection's policy, as a CID over its reference.
    ///
    /// **Version ID only.** The collection ID derivation leaves this unset, so
    /// attaching or amending a policy mints a new version without changing
    /// which collection it is. Like the rest, it reaches the delta only for a
    /// governed collection.
    pub policy_cid: Option<Cid>,
}

/// The policy commitment for a collection's optional policy.
///
/// `None` when there is no policy, and also when its CID cannot be generated:
/// a policy that will not hash leaves the version where it was rather than
/// failing the write.
pub fn policy_commitment(policy: Option<&crate::PolicyDescription>) -> Option<Cid> {
    policy.and_then(|policy| generate_policy_cid(policy).ok())
}

/// A CID over a policy reference, for the version ID.
///
/// It commits to the reference — the policy's ID and resource name — and not
/// to the policy's text, which lives wherever ACP keeps it. Amending a policy
/// there therefore changes the rules without changing this CID.
pub fn generate_policy_cid(policy: &crate::PolicyDescription) -> crate::Result<Cid> {
    let bytes = serde_ipld_dagcbor::to_vec(policy)
        .map_err(|error| crate::SchemaError::CidGeneration(error.to_string()))?;
    let digest = Sha256::digest(&bytes);
    let multihash = Multihash::wrap(SHA2_256_CODE, &digest)
        .map_err(|error| crate::SchemaError::CidGeneration(error.to_string()))?;
    Ok(Cid::new_v1(DAG_CBOR_CODEC, multihash))
}

impl<'a> Commitments<'a> {
    /// The commitments a collection version declares.
    pub fn of(version: &'a crate::CollectionVersion) -> Self {
        Self {
            governance_root: version.governance_root.as_deref(),
            is_branchable: version.is_branchable,
            policy_cid: policy_commitment(version.policy.as_ref()),
        }
    }

    /// Whether the schema declared a root, and so opted its identity into
    /// committing to the rest of these.
    pub fn is_governed(&self) -> bool {
        self.governance_root.is_some()
    }
}

/// Generates a CID for a field definition with priority=1.
///
/// This matches Go's field definition block structure using defra-core's Block type.
/// For collections with multiple fields, use `generate_field_cid_with_priority` instead
/// to match Go's behavior of incrementing priorities (1, 2, 3, ...).
pub fn generate_field_cid(field: &FieldDescription) -> crate::Result<Cid> {
    generate_field_cid_with_priority(field, 1, false)
}

/// Generates a CID for a field definition with a specific priority.
///
/// Go assigns incrementing priorities to each field block (1 for first, 2 for second, etc).
/// The priority affects the CID, so it must match Go's assignment order.
pub fn generate_field_cid_with_priority(
    field: &FieldDescription,
    priority: u64,
    governed: bool,
) -> crate::Result<Cid> {
    generate_field_cid_with_priority_and_heads(field, priority, &[], governed)
}

/// Generates a CID for a field definition with priority and heads.
///
/// During schema patching, existing fields get new blocks with their old CID as a head,
/// matching Go's `AddDelta` behavior where each field's headstore provides previous CIDs.
pub fn generate_field_cid_with_priority_and_heads(
    field: &FieldDescription,
    priority: u64,
    heads: &[Cid],
    governed: bool,
) -> crate::Result<Cid> {
    let delta = field_to_delta_with_priority(field, priority, governed)?;
    let block = Block::new(CrdtDelta::FieldDefinition(delta), heads.to_vec(), vec![]);
    generate_block_cid(&block)
}

/// Generates a CID for a collection definition with priority = num_fields + 1.
///
/// This matches Go's collection definition block structure using defra-core's Block type.
/// Field CIDs are included as DAGLinks (matching Go's behavior where collection blocks
/// link to their field definition blocks).
///
/// NOTE: This calculates priority as (num_fields + 1). For Go AddSchema compatibility,
/// use `generate_collection_cid_with_priority` with priority=1 instead.
pub fn generate_collection_cid(name: &str, field_cids: &[Cid]) -> crate::Result<Cid> {
    // Priority = num_fields + 1 (legacy behavior)
    let priority = (field_cids.len() as u64) + 1;
    generate_collection_cid_with_priority(name, field_cids, priority)
}

/// Generates a CID for a collection definition with a specific priority.
///
/// This matches Go's collection definition block structure using defra-core's Block type.
/// Field CIDs are included as DAGLinks (matching Go's behavior where collection blocks
/// link to their field definition blocks).
///
/// IMPORTANT: Go's actual AddSchema uses priority=1 for ALL blocks (both field and collection),
/// not incrementing priorities. Use priority=1 for Go interoperability.
pub fn generate_collection_cid_with_priority(
    name: &str,
    field_cids: &[Cid],
    priority: u64,
) -> crate::Result<Cid> {
    generate_collection_cid_with_priority_and_heads(name, field_cids, priority, &[])
}

/// Generate a collection CID with specific priority and head CIDs.
///
/// The `name` parameter is optional: Go's Delta only includes the name when it changed.
/// For initial creation, pass `Some(name)`. For patches that only add fields, pass `None`.
pub fn generate_collection_cid_with_priority_and_heads(
    name: &str,
    field_cids: &[Cid],
    priority: u64,
    heads: &[Cid],
) -> crate::Result<Cid> {
    generate_collection_cid_governed(name, field_cids, priority, heads, Commitments::default())
}

/// Generate a collection CID, hashing in the governance root when there is one.
///
/// `None` builds exactly the delta this function built before governance
/// existed, so an ungoverned collection keeps its CID byte for byte. A root
/// changes the delta and so the identity: the same schema under two roots is
/// two collections, and a node that does not know the root is not a replica
/// of either.
pub fn generate_collection_cid_governed(
    name: &str,
    field_cids: &[Cid],
    priority: u64,
    heads: &[Cid],
    commitments: Commitments<'_>,
) -> crate::Result<Cid> {
    generate_collection_cid_full_with_query(
        Some(name),
        field_cids,
        priority,
        heads,
        None,
        None,
        commitments,
    )
}

/// Generate a collection CID with optional name, priority, and head CIDs.
///
/// Go's CollectionDefinition.Delta() only sets Name when it differs from the old version.
/// For patches that only add fields (name unchanged), pass `name=None` to match Go.
pub fn generate_collection_cid_full(
    name: Option<&str>,
    field_cids: &[Cid],
    priority: u64,
    heads: &[Cid],
) -> crate::Result<Cid> {
    generate_collection_cid_full_with_query(
        name,
        field_cids,
        priority,
        heads,
        None,
        None,
        Commitments::default(),
    )
}

/// Generate a collection CID with optional name, priority, head CIDs, and query data.
///
/// For view collections, `query_select` contains the JSON-encoded query definition
/// and `query_transform` contains the lens transform CID.
#[allow(clippy::too_many_arguments)]
pub fn generate_collection_cid_full_with_query(
    name: Option<&str>,
    field_cids: &[Cid],
    priority: u64,
    heads: &[Cid],
    query_select: Option<&[u8]>,
    query_transform: Option<&Cid>,
    commitments: Commitments<'_>,
) -> crate::Result<Cid> {
    let delta = build_collection_delta(name, priority, query_select, query_transform, commitments);

    let links: Vec<DAGLink> = field_cids
        .iter()
        .map(|cid| DAGLink::new("", *cid))
        .collect();

    let block = Block::new(
        CrdtDelta::CollectionDefinition(delta),
        heads.to_vec(),
        links,
    );
    generate_block_cid(&block)
}

/// Helper to build a CollectionDefinitionDeltaPayload with optional fields.
/// Build the delta a collection's identity is a CID over.
///
/// The commitments enter only when the schema declared a governance root:
/// without one the bytes are exactly what they were before governance
/// existed, so no collection anyone has today changes identity.
fn build_collection_delta(
    name: Option<&str>,
    priority: u64,
    query_select: Option<&[u8]>,
    query_transform: Option<&Cid>,
    commitments: Commitments<'_>,
) -> CollectionDefinitionDeltaPayload {
    let mut delta = CollectionDefinitionDeltaPayload::new(priority);
    if let Some(n) = name {
        delta = delta.with_name(n);
    }
    if let Some(qs) = query_select {
        delta = delta.with_query_select(qs.to_vec());
    }
    if let Some(qt) = query_transform {
        delta = delta.with_query_transform(*qt);
    }
    if let Some(root) = commitments.governance_root {
        delta = delta.with_governance_root(root);
        delta = delta.with_branchable(commitments.is_branchable);
        if let Some(policy) = commitments.policy_cid {
            delta = delta.with_policy_cid(policy);
        }
    }
    delta
}

/// Generates a CID from a Block using DAG-CBOR encoding.
fn generate_block_cid(block: &Block) -> crate::Result<Cid> {
    let (cid, _bytes) = generate_block_cid_and_bytes(block)?;
    Ok(cid)
}

/// Generates a CID and serialized bytes from a Block using DAG-CBOR encoding.
fn generate_block_cid_and_bytes(block: &Block) -> crate::Result<(Cid, Vec<u8>)> {
    // Serialize to DAG-CBOR using serde_ipld_dagcbor
    let cbor_bytes = block
        .to_dag_cbor()
        .map_err(|e| crate::SchemaError::CidGeneration(e.to_string()))?;

    let cid = generate_cid_from_bytes(&cbor_bytes)?;
    Ok((cid, cbor_bytes))
}

/// Generates a CID from already-serialized DAG-CBOR bytes.
fn generate_cid_from_bytes(cbor_bytes: &[u8]) -> crate::Result<Cid> {
    // Hash with SHA2-256
    let mut hasher = Sha256::new();
    hasher.update(cbor_bytes);
    let hash_bytes = hasher.finalize();

    // Create multihash (CID crate uses Multihash<64>)
    let mh: Multihash<64> = Multihash::wrap(SHA2_256_CODE, &hash_bytes)
        .map_err(|e| crate::SchemaError::CidGeneration(e.to_string()))?;

    // Create CIDv1 with DAG-CBOR codec
    Ok(Cid::new_v1(DAG_CBOR_CODEC, mh))
}

/// Generate a field definition block with CID and bytes for storage.
///
/// Unlike `generate_field_cid_with_priority_and_heads`, this returns the serialized block
/// bytes so they can be stored in the blockstore for Bitswap retrieval.
pub fn generate_field_block_with_priority_and_heads(
    field: &FieldDescription,
    priority: u64,
    heads: &[Cid],
    governed: bool,
) -> crate::Result<BlockWithCid> {
    let delta = field_to_delta_with_priority(field, priority, governed)?;
    let block = Block::new(CrdtDelta::FieldDefinition(delta), heads.to_vec(), vec![]);
    let (cid, bytes) = generate_block_cid_and_bytes(&block)?;
    Ok(BlockWithCid { cid, bytes })
}

/// Generate a collection definition block with CID and bytes for storage.
///
/// Unlike `generate_collection_cid_full`, this returns the serialized block bytes
/// so they can be stored in the blockstore for Bitswap retrieval.
pub fn generate_collection_block_full(
    name: Option<&str>,
    field_cids: &[Cid],
    priority: u64,
    heads: &[Cid],
) -> crate::Result<BlockWithCid> {
    generate_collection_block_full_with_query(
        name,
        field_cids,
        priority,
        heads,
        None,
        None,
        Commitments::default(),
    )
}

/// Generate a collection definition block (CID + bytes) with optional query data.
///
/// For view collections, includes `query_select` and `query_transform` in the delta
/// so peers can identify the collection as a view after Bitswap sync.
#[allow(clippy::too_many_arguments)]
pub fn generate_collection_block_full_with_query(
    name: Option<&str>,
    field_cids: &[Cid],
    priority: u64,
    heads: &[Cid],
    query_select: Option<&[u8]>,
    query_transform: Option<&Cid>,
    commitments: Commitments<'_>,
) -> crate::Result<BlockWithCid> {
    let delta = build_collection_delta(name, priority, query_select, query_transform, commitments);

    let links: Vec<DAGLink> = field_cids
        .iter()
        .map(|cid| DAGLink::new("", *cid))
        .collect();

    let block = Block::new(
        CrdtDelta::CollectionDefinition(delta),
        heads.to_vec(),
        links,
    );
    let (cid, bytes) = generate_block_cid_and_bytes(&block)?;
    Ok(BlockWithCid { cid, bytes })
}

/// Generates a CollectionSetID CID from the collection CIDs of a circular relation group.
///
/// Matches Go's `saveBlocks()` in `internal/db/collection_id.go`:
/// - Creates a `CollectionSetDelta { priority: 1 }`
/// - Links each collection CID as a DAGLink with empty name
/// - Block::new() sorts links by CID string
/// - DAG-CBOR encode → SHA2-256 → CIDv1
pub fn generate_collection_set_cid(collection_cids: &[Cid]) -> crate::Result<Cid> {
    let delta = CollectionSetDeltaPayload::new(1);
    let links: Vec<DAGLink> = collection_cids
        .iter()
        .map(|cid| DAGLink::new("", *cid))
        .collect();
    let block = Block::new(CrdtDelta::CollectionSet(delta), vec![], links);
    generate_block_cid(&block)
}

/// Convert a FieldDescription to a FieldDefinitionDeltaPayload with a specific priority
fn field_to_delta_with_priority(
    field: &FieldDescription,
    priority: u64,
    governed: bool,
) -> crate::Result<FieldDefinitionDeltaPayload> {
    let mut delta = FieldDefinitionDeltaPayload::new(priority)
        .with_name(&field.name)
        .with_crdt(field.crdt_type.to_u8())
        .with_immutable(governed && field.immutable);

    match &field.kind {
        FieldKind::Scalar(k) => {
            delta = delta.with_scalar_kind(*k as u8);
        }
        FieldKind::ScalarArray(k) => {
            delta = delta.with_scalar_kind(*k as u8);
        }
        FieldKind::Relation { collection_id, .. } => {
            delta = delta.with_collection_id(collection_id);
        }
        FieldKind::SelfRef { relative_id, .. } => {
            if !relative_id.is_empty() {
                let rel_id = relative_id.parse::<i32>().map_err(|e| {
                    crate::SchemaError::CidGeneration(format!(
                        "Invalid relative_id '{}': {}",
                        relative_id, e
                    ))
                })?;
                delta = delta.with_relative_id(rel_id);
            }
        }
        FieldKind::Named { .. } => {}
    }

    Ok(delta)
}
