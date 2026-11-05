use super::{compute_document_blocks, DocRef};
use defra_core::block::{Block, Signature};
use defra_core::cid_encoding::cid_bytes;
use defra_core::signing::{SigningConfig, SigningKeyType};
use document::Document;
use schema::{CType, FieldKind};

/// The client's own signed create graph, prepared before any server challenge.
/// The graph is built from the caller's trusted complete schema and intended
/// document; neither a remote schema version nor challenge is an authority.
/// Existing block signatures can be replayed outside the HTTP conversation.
pub struct ClientCreateSignatures {
    children: Vec<Vec<u8>>,
    child_signatures: Vec<Vec<u8>>,
    composite: Vec<u8>,
    composite_signature: Vec<u8>,
    phase: Phase,
    collection: String,
    input: serde_json::Value,
    public_key_hex: String,
    doc_id: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Children,
    Composite,
    Complete,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientCreateError {
    #[error("client signing requires a new unencrypted document without generated fields")]
    UnsupportedDocument,
    #[error("client signing currently requires a local Ed25519 key")]
    UnsupportedSigner,
    #[error("client create graph could not be built: {0}")]
    Build(String),
    #[error("client create graph contains an invalid signature or block")]
    InvalidGraph,
    #[error("signing wave does not match the client's approved create graph")]
    UnexpectedWave,
}

impl ClientCreateSignatures {
    /// Prepare signatures entirely on the client from an approved document.
    /// This bounded form supports non-null LWW scalar and scalar-list fields,
    /// including immutable fields. The caller supplies the trusted schema in
    /// `doc`; this function neither fetches nor authenticates it.
    pub fn new(
        crdts: &crdt::Crdts,
        doc: &Document,
        signer: &SigningConfig,
    ) -> Result<Self, ClientCreateError> {
        if signer.key_type != SigningKeyType::Ed25519
            || signer.remote_signer.is_some()
            || signer.private_key_bytes.is_empty()
        {
            return Err(ClientCreateError::UnsupportedSigner);
        }
        let schema = doc.collection();
        if doc.id().is_some()
            || doc.head().is_some()
            || doc.is_deleted()
            || schema.is_branchable
            || !schema.vector_embeddings.is_empty()
            || schema.fields.iter().any(|field| {
                field.default_value.is_some() && !doc.values().contains_key(&field.name)
            })
            || doc.values().iter().any(|(name, value)| {
                let Some(field) = schema.field_by_name(name) else {
                    return true;
                };
                value.value().is_nil()
                    || field.crdt_type != CType::LwwRegister
                    || !matches!(field.kind, FieldKind::Scalar(_) | FieldKind::ScalarArray(_))
                    || doc.pending_op(name).is_some()
            })
        {
            return Err(ClientCreateError::UnsupportedDocument);
        }
        let blocks =
            compute_document_blocks(crdts, doc, schema, DocRef::new(0, 0), None, Some(signer))
                .map_err(ClientCreateError::Build)?;
        let public =
            crypto::public_key_from_string(crypto::KeyType::Ed25519, &signer.public_key_hex)
                .map_err(|_| ClientCreateError::InvalidGraph)?;
        let extract = |cid: &cid::Cid| -> Result<(Vec<u8>, Vec<u8>), ClientCreateError> {
            let bytes = blocks
                .blockstore_entries
                .iter()
                .find(|(key, _)| key.as_slice() == cid_bytes(cid).as_slice())
                .map(|(_, bytes)| bytes)
                .ok_or(ClientCreateError::InvalidGraph)?;
            let mut block =
                Block::from_dag_cbor(bytes).map_err(|_| ClientCreateError::InvalidGraph)?;
            let signature_cid = block
                .signature
                .take()
                .ok_or(ClientCreateError::InvalidGraph)?;
            let signature_bytes = blocks
                .blockstore_entries
                .iter()
                .find(|(key, _)| key.as_slice() == cid_bytes(&signature_cid).as_slice())
                .map(|(_, bytes)| bytes)
                .ok_or(ClientCreateError::InvalidGraph)?;
            let signature = Signature::from_dag_cbor(signature_bytes)
                .map_err(|_| ClientCreateError::InvalidGraph)?;
            let data = block
                .to_dag_cbor()
                .map_err(|_| ClientCreateError::InvalidGraph)?;
            if signature.header.identity != signer.public_key_hex.as_bytes()
                || !public
                    .verify(&data, &signature.value)
                    .map_err(|_| ClientCreateError::InvalidGraph)?
            {
                return Err(ClientCreateError::InvalidGraph);
            }
            Ok((data, signature.value))
        };
        let mut children = Vec::new();
        let mut child_signatures = Vec::new();
        for cid in &blocks.block_result.field_cids {
            let (block, signature) = extract(cid)?;
            children.push(block);
            child_signatures.push(signature);
        }
        let (composite, composite_signature) = extract(&blocks.block_result.cid)?;
        let phase = if children.is_empty() {
            Phase::Composite
        } else {
            Phase::Children
        };
        let input =
            serde_json::to_value(doc.to_map().map_err(|_| ClientCreateError::InvalidGraph)?)
                .map_err(|_| ClientCreateError::InvalidGraph)?;
        Ok(Self {
            collection: schema.name.clone(),
            input,
            public_key_hex: signer.public_key_hex.clone(),
            doc_id: blocks.block_result.doc_id.clone(),
            children,
            child_signatures,
            composite,
            composite_signature,
            phase,
        })
    }

    pub fn matches_input(&self, collection: &str, input: &serde_json::Value) -> bool {
        self.collection == collection && self.input == *input
    }

    pub fn public_key_hex(&self) -> &str {
        &self.public_key_hex
    }

    pub fn doc_id(&self) -> &str {
        &self.doc_id
    }

    pub fn is_complete(&self) -> bool {
        self.phase == Phase::Complete
    }

    pub fn child_blocks(&self) -> &[Vec<u8>] {
        &self.children
    }

    pub fn composite_block(&self) -> &[u8] {
        &self.composite
    }

    /// Return only signatures over the complete expected wave; reject extra,
    /// missing, altered or repeated blocks before releasing any signature.
    pub fn respond(&mut self, wave: &[Vec<u8>]) -> Result<Vec<Vec<u8>>, ClientCreateError> {
        if self.phase == Phase::Composite {
            if wave.len() != 1 || wave[0] != self.composite {
                return Err(ClientCreateError::UnexpectedWave);
            }
            self.phase = Phase::Complete;
            return Ok(vec![self.composite_signature.clone()]);
        }
        if self.phase != Phase::Children || wave.len() != self.children.len() {
            return Err(ClientCreateError::UnexpectedWave);
        }
        let mut used = vec![false; self.children.len()];
        let mut result = Vec::with_capacity(wave.len());
        for requested in wave {
            let index = self
                .children
                .iter()
                .enumerate()
                .find(|(index, expected)| !used[*index] && *expected == requested)
                .map(|(index, _)| index)
                .ok_or(ClientCreateError::UnexpectedWave)?;
            used[index] = true;
            result.push(self.child_signatures[index].clone());
        }
        self.phase = Phase::Composite;
        Ok(result)
    }
}
