//! IPLD block builder for document mutations.
//!
//! Creates proper Block structures with CRDT delta payloads for P2P synchronization.
//! Matches Go DefraDB's block format for wire compatibility.
//!
//! This module provides two main functions:
//! - `build_blocks_from_document`: For P2P broadcast (uses external blockstore)
//! - `write_document_blocks`: For FFI/local storage (uses transaction stores)

mod batch_signing;
mod build;
pub mod collection;
mod compute;
mod immutable;
mod write;

pub(crate) use batch_signing::collect_on_commit;
pub use build::build_blocks_from_document;
pub use collection::write_collection_block;
pub(crate) use collection::write_collection_block_with_inputs;
pub(crate) use compute::compute_document_blocks_with_inputs;
pub use compute::{compute_document_blocks, insert_computed_blocks, ComputedBlocks};
pub(crate) use write::inherited_encryption_policy;
pub use write::{write_delete_block, write_document_blocks};
pub(crate) use write::{write_delete_block_with_inputs, write_document_blocks_with_inputs};

use immutable::{compute_immutable_block, immutable_field_names, immutable_for_pass};

use bytes::Bytes;
use rapidhash::{HashMapExt, RapidHashMap};

use tracing::warn;

use cid::Cid;
use crypto::PrivateKey;
use datastore::NamespaceView;
use defra_core::block::{
    generate_cid_from_bytes, Block, CollectionDeltaPayload, CompositeDeltaPayload, CrdtDelta,
    DAGLink, Encryption, Signature, SignatureHeader,
};
use defra_core::cid_encoding::cid_bytes;
use defra_core::encryption::EncryptionConfig;
use defra_core::signing::SigningConfig;
use document::{DocID, Document, NormalValue};
use schema::CollectionVersion;
use storage::corekv::Key;
use storage::keys::doc_id_index::DocRef;
use storage::keys::headstore::HeadstorePriorityKey;

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("{0}")]
    Message(String),
    #[error("external signature input required")]
    NeedsInput(query::prepared::PreparedInputRequest),
}

impl From<String> for BuildError {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

impl From<query::error::QueryError> for BuildError {
    fn from(value: query::error::QueryError) -> Self {
        match value {
            query::error::QueryError::NeedsInput(request) => Self::NeedsInput(request),
            other => Self::Message(other.to_string()),
        }
    }
}

impl BuildError {
    pub fn into_query_error(self) -> query::error::QueryError {
        match self {
            Self::NeedsInput(request) => query::error::QueryError::NeedsInput(request),
            Self::Message(message) => query::error::QueryError::execution(message),
        }
    }

    /// [`into_query_error`](Self::into_query_error) with `context`, naming the
    /// write, before a failure's message. A missing input stays a request for
    /// it, so the caller can fetch it and run the write again.
    pub fn into_query_error_in(self, context: &str) -> query::error::QueryError {
        match self {
            Self::NeedsInput(request) => query::error::QueryError::NeedsInput(request),
            Self::Message(message) => {
                query::error::QueryError::execution(format!("{context}: {message}"))
            }
        }
    }
}

/// Node-local storage identity of a document.
///
/// Storage keys are built from the doc short ID; the public DocID exists
/// only after the genesis composite block CID is known.
#[derive(Debug, Clone, Copy)]
pub struct DocStorageIdentity {
    pub collection_short_id: u32,
    pub doc_short_id: u64,
}

impl DocStorageIdentity {
    pub fn new(collection_short_id: u32, doc_short_id: u64) -> Self {
        Self {
            collection_short_id,
            doc_short_id,
        }
    }

    /// Encoded DocRef, used as the encryption key derivation identity
    /// (mirrors Go's `EncodeDocRef` encryptor cache key).
    pub fn doc_ref_bytes(&self) -> Vec<u8> {
        DocRef::new(self.collection_short_id, self.doc_short_id).encode()
    }
}

/// Derive the public DocID string from the genesis composite block CID.
pub fn derive_doc_id(genesis_cid: &Cid) -> String {
    DocID::new_v0(*genesis_cid).to_string()
}

pub(crate) fn encrypt_delta(plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, String> {
    let (ciphertext, _nonce) = crypto::encryption::aes::encrypt_aes(plaintext, key, &[], true)
        .map_err(|e| format!("encryption failed: {}", e))?;
    Ok(ciphertext)
}

/// Keep locally generated keys stable for one request's preparation passes.
pub(crate) fn encryption_key_prepared(
    doc_ref: &[u8],
    field: Option<&str>,
    inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<[u8; 32], String> {
    let Some(inputs) = inputs else {
        return Ok(defra_core::encryption::generate_encryption_key_for(
            doc_ref, field,
        ));
    };
    let exact = serde_json::to_vec(&("local-encryption-key-v1", doc_ref, field))
        .map_err(|error| error.to_string())?;
    let request = query::prepared::PreparedInputRequest::new(
        query::prepared::PreparedInputKind::Encryption,
        exact,
    );
    if let Some(key) = inputs.get(&request) {
        return key
            .as_ref()
            .try_into()
            .map_err(|_| "invalid prepared local encryption key".into());
    }
    let key = defra_core::encryption::generate_encryption_key_for(doc_ref, field);
    inputs
        .insert(request, std::sync::Arc::<[u8]>::from(key))
        .as_ref()
        .try_into()
        .map_err(|_| "invalid prepared local encryption key".into())
}

/// Preserve a randomized ciphertext across transaction re-entry when a later
/// prepared input (such as a remote signature) suspends the attempt.
pub(crate) fn encrypt_delta_prepared(
    plaintext: &[u8],
    key: &[u8; 32],
    inputs: &query::prepared::PreparedInputMemo,
) -> Result<Vec<u8>, String> {
    let mut exact = Vec::with_capacity(40 + plaintext.len());
    exact.extend_from_slice(b"AES-GCM-delta-v1");
    exact.extend_from_slice(key);
    exact.extend_from_slice(&(plaintext.len() as u64).to_be_bytes());
    exact.extend_from_slice(plaintext);
    let request = query::prepared::PreparedInputRequest::new(
        query::prepared::PreparedInputKind::Encryption,
        exact,
    );
    if let Some(ciphertext) = inputs.get(&request) {
        return Ok(ciphertext.to_vec());
    }
    let ciphertext = encrypt_delta(plaintext, key)?;
    // Every use in the request shares the first ciphertext.
    Ok(inputs
        .insert(request, std::sync::Arc::<[u8]>::from(ciphertext))
        .to_vec())
}

/// Compute a signature block without writing to storage.
///
/// Pure function: returns `(sig_cid, sig_cbor_bytes)` for the caller to store.
/// Returns `None` for field blocks with priority > 1 (not signed per Go behavior).
pub fn compute_signature(
    block: &Block,
    signer: &SigningConfig,
) -> Result<Option<(Cid, Bytes)>, String> {
    // Only sign first field blocks (priority <= 1) and composite blocks.
    // Higher-priority field blocks are not signed: their integrity is
    // guaranteed by the signature on the parent composite block.
    let is_field = matches!(&block.delta, CrdtDelta::Lww(_) | CrdtDelta::Counter(_));
    if is_field && block.delta.priority() > 1 {
        return Ok(None);
    }

    // Serialize the block (without signature) to get the bytes to sign
    let block_bytes = block
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode block for signing: {}", e))?;

    let sig_type: defra_core::block::SignatureType = signer.key_type.into();

    // A Go peer refuses a signature type it cannot map to a key type, so a block
    // signed with a Rust-only type replicates between Rust nodes and is rejected
    // by Go ones. Emitting one is a deployment decision, so it is refused unless
    // the node has said it accepts a partitioned network.
    if !sig_type.is_go_verifiable()
        && !defra_core::block::go_verifiable_policy::non_go_verifiable_signing_allowed()
    {
        return Err(format!(
            "refusing to sign a block with {sig_type:?} ({}): Go peers cannot verify it and \
             will reject the block during replication. Set {}=1 to allow it on this node.",
            signer.key_type,
            defra_core::block::go_verifiable_policy::ALLOW_ENV,
        ));
    }
    if !sig_type.is_go_verifiable() {
        warn!(
            signature_type = ?sig_type,
            key_type = %signer.key_type,
            "signing a block Go peers cannot verify; it will be rejected when \
             replicating to a Go node"
        );
    }

    let sig_bytes = if let Some(remote) = signer.remote_signer.as_ref() {
        remote.sign_sync(&block_bytes, signer.signing_authorization.as_ref())?
    } else {
        match signer.key_type {
            defra_core::signing::SigningKeyType::Ed25519 => {
                let private_key = crypto::Ed25519PrivateKey::from_bytes(&signer.private_key_bytes)
                    .map_err(|e| format!("Failed to load Ed25519 private key: {}", e))?;
                private_key
                    .sign(&block_bytes)
                    .map_err(|e| format!("Failed to sign block: {}", e))?
            }
            defra_core::signing::SigningKeyType::Secp256k1 => {
                let private_key =
                    crypto::Secp256k1PrivateKey::from_bytes(&signer.private_key_bytes)
                        .map_err(|e| format!("Failed to load secp256k1 private key: {}", e))?;
                private_key
                    .sign(&block_bytes)
                    .map_err(|e| format!("Failed to sign block: {}", e))?
            }
            defra_core::signing::SigningKeyType::BlsAugV1 => {
                return Err("BLS signing requires a remote signer".to_string());
            }
            defra_core::signing::SigningKeyType::Secp256r1 => {
                return Err(
                    "secp256r1 signing requires a remote signer: a Secure Enclave key cannot be \
                     exported"
                        .to_string(),
                );
            }
            other => {
                return Err(format!("Unsupported key type for signing: {}", other));
            }
        }
    };

    // Create signature block.
    // Go uses `[]byte(fullIdent.PublicKey().String())` for identity,
    // which is the hex-encoded public key string as bytes.
    let signature = Signature::new(
        SignatureHeader::new(sig_type, signer.public_key_hex.as_bytes().to_vec()),
        sig_bytes,
    );

    let sig_cbor = signature
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode signature block: {}", e))?;
    let sig_cid = generate_cid_from_bytes(&sig_cbor)
        .map_err(|e| format!("Failed to generate signature CID: {}", e))?;

    Ok(Some((sig_cid, sig_cbor.into())))
}

/// Compute or request a signature using the exact-input memo for the active
/// autocommit request. Local signing and non-requested paths retain the normal
/// synchronous behavior.
fn compute_signature_prepared(
    block: &Block,
    signer: &SigningConfig,
    inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<Option<(Cid, Bytes)>, BuildError> {
    if signer.remote_signer.is_none() || inputs.is_none() {
        return compute_signature(block, signer).map_err(BuildError::Message);
    }
    let is_field = matches!(&block.delta, CrdtDelta::Lww(_) | CrdtDelta::Counter(_));
    if is_field && block.delta.priority() > 1 {
        return Ok(None);
    }
    let data = block
        .to_dag_cbor()
        .map_err(|e| BuildError::Message(format!("Failed to encode block for signing: {e}")))?;
    let input = defra_core::signing::RemoteSignatureInput {
        data,
        key_type: signer.key_type,
        public_key_hex: signer.public_key_hex.clone(),
        authorization: signer.signing_authorization.clone(),
    };
    let request = query::prepared::PreparedInputRequest::new(
        query::prepared::PreparedInputKind::Signature,
        input.encode().map_err(BuildError::Message)?,
    );
    let sig_type: defra_core::block::SignatureType = signer.key_type.into();
    if !sig_type.is_go_verifiable()
        && !defra_core::block::go_verifiable_policy::non_go_verifiable_signing_allowed()
    {
        return Err(BuildError::Message(format!(
            "refusing to sign a block with {sig_type:?} ({}): Go peers cannot verify it and will reject the block during replication. Set {}=1 to allow it on this node.",
            signer.key_type,
            defra_core::block::go_verifiable_policy::ALLOW_ENV,
        )));
    }
    if !sig_type.is_go_verifiable() {
        warn!(
            signature_type = ?sig_type,
            key_type = %signer.key_type,
            "signing a block Go peers cannot verify; it will be rejected when replicating to a Go node"
        );
    }
    // A miss is noted and the pass goes on: every signature of a wave is
    // asked for at once.
    let response = inputs
        .expect("checked above")
        .get_or_want(&request)
        .ok_or(BuildError::NeedsInput(request))?;
    let signature = Signature::new(
        SignatureHeader::new(sig_type, signer.public_key_hex.as_bytes().to_vec()),
        response.to_vec(),
    );
    let sig_cbor = signature
        .to_dag_cbor()
        .map_err(|e| BuildError::Message(format!("Failed to encode signature block: {e}")))?;
    let sig_cid = generate_cid_from_bytes(&sig_cbor)
        .map_err(|e| BuildError::Message(format!("Failed to generate signature CID: {e}")))?;
    Ok(Some((sig_cid, sig_cbor.into())))
}

/// Sign a block and store the signature as a separate IPLD block.
///
/// Delegates to `compute_signature()` for the pure computation, then writes
/// the signature block to blockstore. The caller must then set
/// `block.signature = Some(sig_cid)` and re-serialize.
pub(crate) async fn sign_block(
    block: &Block,
    signer: &SigningConfig,
    blockstore: &NamespaceView,
    inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<Option<Cid>, BuildError> {
    let Some((sig_cid, sig_cbor)) = compute_signature_prepared(block, signer, inputs)? else {
        return Ok(None);
    };

    blockstore
        .set(&cid_bytes(&sig_cid), &sig_cbor)
        .await
        .map_err(|e| BuildError::Message(format!("Failed to store signature block: {}", e)))?;

    Ok(Some(sig_cid))
}

/// Result of building blocks from a document mutation.
#[derive(Debug, Clone)]
pub struct BlockResult {
    /// The CID of the composite (root) block
    pub cid: Cid,
    /// The raw composite block bytes (DAG-CBOR encoded)
    pub block: Bytes,
    /// The document ID
    pub doc_id: String,
    /// CIDs of all field blocks created
    pub field_cids: Vec<Cid>,
    /// CIDs of all Encryption blocks created (mapped to the owning DocID
    /// alongside composite + field blocks, mirroring Go's save()).
    pub encryption_cids: Vec<Cid>,
}

/// The CRDT type of a document field, which decides what its block carries.
pub(crate) fn field_crdt_of(
    doc: &Document,
    field_name: &str,
) -> Result<&'static dyn crdt::FieldCrdt, String> {
    let ctype = doc
        .crdt_type(field_name)
        .ok_or_else(|| format!("field {field_name} is not in the collection definition"))?;
    crdt::field_crdt(ctype)
        .ok_or_else(|| format!("field {field_name} has no CRDT type that writes blocks ({ctype})"))
}

/// Encode a NormalValue as CBOR bytes.
pub fn encode_value_as_cbor(value: &NormalValue) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(value.cbor_size_hint());
    ciborium::into_writer(value, &mut bytes)
        .map_err(|e| format!("Failed to encode value as CBOR: {}", e))?;
    Ok(bytes)
}

/// Encode a priority as a varint (matching Go's binary.PutUvarint).
pub(crate) fn encode_priority_varint(priority: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(10); // Max varint64 is 10 bytes
    let mut n = priority;
    while n >= 0x80 {
        buf.push((n as u8) | 0x80);
        n >>= 7;
    }
    buf.push(n as u8);
    buf
}

/// Decode a varint to priority (matching Go's binary.Uvarint).
pub fn decode_priority_varint(buf: &[u8]) -> u64 {
    let mut n: u64 = 0;
    let mut shift: u32 = 0;
    for &byte in buf {
        if shift >= 64 {
            return 0; // Overflow protection
        }
        n |= ((byte & 0x7f) as u64) << shift;
        if byte < 0x80 {
            return n;
        }
        shift += 7;
    }
    n
}

pub(crate) fn priority_index_key(doc_short_id: u64, priority: u64, cid: Cid) -> Vec<u8> {
    HeadstorePriorityKey::new(doc_short_id, priority, cid).bytes()
}

/// A document's live heads, read with one point read of its head set.
#[derive(Clone, PartialEq)]
pub struct DocHeadsSnapshot {
    max_priority: u64,
    max_priority_by_field: RapidHashMap<String, u64>,
    heads_by_field: RapidHashMap<String, Vec<Cid>>,
}

impl DocHeadsSnapshot {
    /// Load all heads for a document: one point read, validated by nothing.
    pub async fn load(headstore: &NamespaceView, doc_short_id: u64) -> Result<Self, String> {
        let set = crate::block::heads::document::load(headstore, doc_short_id)
            .await
            .map_err(|e| format!("Failed to read document heads: {}", e))?;
        let mut snapshot = Self {
            max_priority: 0,
            max_priority_by_field: RapidHashMap::new(),
            heads_by_field: RapidHashMap::new(),
        };
        // A name is `{field}/{cid}`; names ascend, so each field's heads are
        // already in CID text order, as Go's Block.New() sorts them.
        for (name, priority) in set.live() {
            let Some((field, cid)) = crate::block::heads::document::parse_name(name) else {
                continue;
            };
            snapshot.max_priority = snapshot.max_priority.max(priority);
            let max = snapshot
                .max_priority_by_field
                .entry(field.to_string())
                .or_default();
            *max = (*max).max(priority);
            snapshot
                .heads_by_field
                .entry(field.to_string())
                .or_default()
                .push(cid);
        }
        Ok(snapshot)
    }

    pub fn max_priority(&self) -> u64 {
        self.max_priority
    }

    pub fn field_max_priority(&self, field_id: &str) -> u64 {
        self.max_priority_by_field
            .get(field_id)
            .copied()
            .unwrap_or_default()
    }

    /// Heads for a specific field (empty if none).
    pub fn field_heads(&self, field_id: &str) -> Vec<Cid> {
        self.heads_by_field
            .get(field_id)
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod prepared_signature_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingSigner(AtomicUsize);

    impl defra_core::signing::RemoteSigner for CountingSigner {
        fn sign_sync(
            &self,
            data: &[u8],
            _authorization: Option<&defra_core::signing::SigningAuthorization>,
        ) -> Result<Vec<u8>, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(data.iter().rev().copied().collect())
        }
    }

    fn block(parent: Option<Cid>) -> Block {
        let payload = CompositeDeltaPayload {
            schema_version_id: "schema-v1".into(),
            priority: 2,
            status: 0,
        };
        Block::new(
            CrdtDelta::Composite(payload),
            parent.into_iter().collect(),
            Vec::new(),
        )
    }

    #[test]
    fn remote_signature_input_is_exact_and_reused_only_for_same_head_bytes() {
        let remote = std::sync::Arc::new(CountingSigner(AtomicUsize::new(0)));
        let signer = SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: Vec::new(),
            public_key_bytes: vec![1, 2, 3],
            public_key_hex: "010203".into(),
            remote_signer: Some(remote.clone()),
            signing_authorization: Some(defra_core::signing::SigningAuthorization::Decision {
                decision_id: "decision-9".into(),
            }),
        };
        let memo = query::prepared::PreparedInputMemo::default();
        let parent_a = generate_cid_from_bytes(b"parent-a").unwrap();
        let first = block(Some(parent_a));
        let request = match compute_signature_prepared(&first, &signer, Some(&memo)) {
            Err(BuildError::NeedsInput(request)) => request,
            other => panic!("expected deferred signature, got {other:?}"),
        };
        // Reaching NeedsInput leaves the transaction attempt before invoking
        // the remote signer.
        assert_eq!(remote.0.load(Ordering::SeqCst), 0);
        let input =
            defra_core::signing::RemoteSignatureInput::decode(request.exact_request()).unwrap();
        assert_eq!(input.data, first.to_dag_cbor().unwrap());
        let changed_authorization = SigningConfig {
            key_type: signer.key_type,
            private_key_bytes: signer.private_key_bytes.clone(),
            public_key_bytes: signer.public_key_bytes.clone(),
            public_key_hex: signer.public_key_hex.clone(),
            remote_signer: signer.remote_signer.clone(),
            signing_authorization: Some(defra_core::signing::SigningAuthorization::Decision {
                decision_id: "decision-10".into(),
            }),
        };
        assert!(input.resolve(&changed_authorization).is_err());
        assert_eq!(remote.0.load(Ordering::SeqCst), 0);
        let signature = input.resolve(&signer).unwrap();
        assert_eq!(remote.0.load(Ordering::SeqCst), 1);
        memo.insert(request.clone(), std::sync::Arc::<[u8]>::from(signature));
        assert!(compute_signature_prepared(&first, &signer, Some(&memo)).is_ok());
        assert_eq!(remote.0.load(Ordering::SeqCst), 1);

        let parent_b = generate_cid_from_bytes(b"parent-b").unwrap();
        let changed = block(Some(parent_b));
        let changed_request = match compute_signature_prepared(&changed, &signer, Some(&memo)) {
            Err(BuildError::NeedsInput(request)) => request,
            other => panic!("changed head must request a new signature, got {other:?}"),
        };
        assert_ne!(request, changed_request);
    }

    #[test]
    fn changed_kms_encryption_scope_requests_a_fresh_key() {
        let identity = DocStorageIdentity::new(1, 2);
        let mut doc = Document::new(crate::common::schema::open_collection(&["secret"]));
        doc.set("secret", "value").unwrap();
        let mut prepared = document::WritePreparation::new(1, 2);
        prepared.set_requires_kms_keys(true);
        prepared.insert_scoped_key("secret".into(), Cid::default(), [7; 32], None);
        doc.set_write_preparation(std::sync::Arc::new(prepared));

        // Changing document-level encryption to field-level encryption
        // changes the KMS scope, so the previously prepared key cannot match.
        let config = EncryptionConfig {
            encrypt_doc: false,
            encrypt_fields: vec!["secret".into()],
        };
        let memo = query::prepared::PreparedInputMemo::default();
        let schema = CollectionVersion::new("Users", "schema-v1", "users", vec![]);
        let error = compute_document_blocks_with_inputs(
            &doc,
            &schema,
            identity,
            Some(&config),
            None,
            Some(&memo),
        )
        .unwrap_err();
        let BuildError::NeedsInput(request) = error else {
            panic!("expected fresh KMS input request, got {error:?}");
        };
        let operation: crate::write::prepare::PreparedKmsOperation =
            serde_json::from_slice(request.exact_request()).unwrap();
        assert!(matches!(
            operation,
            crate::write::prepare::PreparedKmsOperation::Generate {
                doc_id,
                field: Some(field),
            } if doc_id == hex::encode(identity.doc_ref_bytes()) && field == "secret"
        ));
    }
}
