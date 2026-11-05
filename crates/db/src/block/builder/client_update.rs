use std::sync::Arc;

use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta, DAGLink, Signature};
use defra_core::signing::{SigningConfig, SigningKeyType};
use document::Document;
use rapidhash::{HashMapExt, RapidHashMap};
use schema::{CType, CollectionVersion, FieldKind, ScalarKind};

use super::{client_waves::ClientWaves, compute_signature, ClientUpdateSnapshot};

#[derive(Debug, thiserror::Error)]
pub enum ClientUpdateError {
    #[error("the authenticated update history query failed")]
    HistoryRead,
    #[error("the update snapshot is incomplete, encrypted, or does not belong to the document")]
    InvalidSnapshot,
    #[error("client signing requires an unencrypted scalar LWW update")]
    UnsupportedUpdate,
    #[error("client signing requires a local Ed25519 key")]
    UnsupportedSigner,
    #[error("client update graph could not be built: {0}")]
    Build(String),
    #[error("client update graph contains an invalid signature or block")]
    InvalidGraph,
    #[error("signing wave does not match the client's approved update graph")]
    UnexpectedWave,
}

/// Signatures prepared from a trusted schema, exact patch, and independently
/// verified document history. A signature is an ordinary DefraDB block
/// signature and can be replayed outside this conversation.
pub struct ClientUpdateSignatures {
    collection: String,
    schema: Arc<CollectionVersion>,
    input: serde_json::Value,
    doc_id: String,
    public_key_hex: String,
    waves: ClientWaves,
}

impl ClientUpdateSignatures {
    fn normalize_input(
        crdts: &crdt::Crdts,
        schema: &CollectionVersion,
        input: &serde_json::Value,
    ) -> Option<Document> {
        let input = input.as_object()?;
        if input.is_empty() || input.iter().any(|(name, value)| {
            let Some(field) = schema.field_by_name(name) else { return true; };
            field.immutable || field.relation_name.is_some() || field.crdt_type != CType::LwwRegister
                || !matches!(field.kind, FieldKind::Scalar(_) | FieldKind::ScalarArray(_))
                || (matches!(&field.kind, FieldKind::Scalar(kind) if kind.base_kind() == ScalarKind::NillableDateTime)
                    && value.as_str() == Some("UTC_NOW"))
                || (matches!(&field.kind, FieldKind::ScalarArray(kind) if kind.element_kind() == ScalarKind::NillableDateTime)
                    && value.as_array().is_some_and(|values| values.iter().any(|value| value.as_str() == Some("UTC_NOW"))))
        }) {
            return None;
        }
        let fields = input
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let update = query::plan::mutation::UpdateInput {
            fields,
            ops: RapidHashMap::new(),
        };
        let mut doc = Document::new(schema.clone());
        update.apply_to(crdts, &mut doc, Some(schema)).ok()?;
        Some(doc)
    }

    /// Build the complete graph before the server can request a signature.
    pub fn from_input(
        crdts: &crdt::Crdts,
        schema: &CollectionVersion,
        doc_id: &str,
        input: &serde_json::Value,
        snapshot: &ClientUpdateSnapshot,
        signer: &SigningConfig,
    ) -> Result<Self, ClientUpdateError> {
        if signer.key_type != SigningKeyType::Ed25519
            || signer.remote_signer.is_some()
            || signer.private_key_bytes.is_empty()
        {
            return Err(ClientUpdateError::UnsupportedSigner);
        }
        if doc_id != snapshot.doc_id
            || snapshot.versions.len() != 1
            || !snapshot.versions.contains(&schema.version_id)
            || schema.is_branchable
            || snapshot.values.keys().any(|name| {
                schema.field_by_name(name).is_none_or(|field| {
                    field.crdt_type != CType::LwwRegister
                        || field.relation_name.is_some()
                        || !matches!(field.kind, FieldKind::Scalar(_) | FieldKind::ScalarArray(_))
                })
            })
            || schema.field_by_name("C").is_some()
            || !schema.vector_embeddings.is_empty()
            || schema.downsample_source.is_some()
        {
            return Err(ClientUpdateError::UnsupportedUpdate);
        }
        let doc = Self::normalize_input(crdts, schema, input)
            .ok_or(ClientUpdateError::UnsupportedUpdate)?;
        let public =
            crypto::public_key_from_string(crypto::KeyType::Ed25519, &signer.public_key_hex)
                .map_err(|_| ClientUpdateError::InvalidGraph)?;
        let sign = |block: &mut Block| -> Result<Option<Vec<u8>>, ClientUpdateError> {
            let bytes = block
                .to_dag_cbor()
                .map_err(|_| ClientUpdateError::InvalidGraph)?;
            let Some((cid, encoded)) =
                compute_signature(block, signer).map_err(ClientUpdateError::Build)?
            else {
                return Ok(None);
            };
            let signature =
                Signature::from_dag_cbor(&encoded).map_err(|_| ClientUpdateError::InvalidGraph)?;
            if signature.header.identity != signer.public_key_hex.as_bytes()
                || !public
                    .verify(&bytes, &signature.value)
                    .map_err(|_| ClientUpdateError::InvalidGraph)?
            {
                return Err(ClientUpdateError::InvalidGraph);
            }
            block.signature = Some(cid);
            Ok(Some(signature.value))
        };
        let mut children = Vec::new();
        let mut child_signatures = Vec::new();
        let mut links = Vec::new();
        for (name, value) in doc.values() {
            let field = schema
                .field_by_name(name)
                .ok_or(ClientUpdateError::UnsupportedUpdate)?;
            let crdt = crdts
                .field(field.crdt_type)
                .ok_or(ClientUpdateError::UnsupportedUpdate)?;
            let current = snapshot
                .values
                .get(name)
                .map(|(priority, data)| {
                    let state = crdt
                        .block_state(field, *priority, data)
                        .map_err(|error| ClientUpdateError::Build(error.to_string()))?;
                    crdt.materialize(state.part())
                        .ok_or(ClientUpdateError::InvalidSnapshot)
                })
                .transpose()?;
            if current
                .as_ref()
                .is_some_and(|current| !current.is_nil() && current == value.value())
            {
                continue;
            }
            let data = crdt
                .block_data(value.value(), doc.pending_op(name))
                .map_err(|error| ClientUpdateError::Build(error.to_string()))?
                .ok_or(ClientUpdateError::InvalidGraph)?;
            let priority = snapshot
                .priorities
                .get(name)
                .copied()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or(ClientUpdateError::InvalidSnapshot)?;
            let delta = crdt.delta(crdt::DeltaParts {
                field_name: name,
                priority,
                collection_version_id: &schema.version_id,
                nonce: 0,
                data,
            });
            let mut block = Block::new(
                delta,
                snapshot.heads.get(name).cloned().unwrap_or_default(),
                vec![],
            );
            let bytes = block
                .to_dag_cbor()
                .map_err(|_| ClientUpdateError::InvalidGraph)?;
            if let Some(signature) = sign(&mut block)? {
                children.push(bytes);
                child_signatures.push(signature);
            }
            links.push(DAGLink::new(
                name.clone(),
                block
                    .generate_cid()
                    .map_err(|_| ClientUpdateError::InvalidGraph)?,
            ));
        }
        if links.is_empty() {
            let input =
                serde_json::to_value(doc.to_map().map_err(|_| ClientUpdateError::InvalidGraph)?)
                    .map_err(|_| ClientUpdateError::InvalidGraph)?;
            return Ok(Self {
                collection: schema.name.clone(),
                schema: Arc::new(schema.clone()),
                input,
                doc_id: doc_id.to_owned(),
                public_key_hex: signer.public_key_hex.clone(),
                waves: ClientWaves::complete(),
            });
        }
        let priority = snapshot
            .max_priority
            .checked_add(1)
            .ok_or(ClientUpdateError::InvalidSnapshot)?;
        let mut composite = Block::new(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: schema.version_id.clone(),
                priority,
                status: 1,
            }),
            snapshot
                .heads
                .get("C")
                .cloned()
                .ok_or(ClientUpdateError::InvalidSnapshot)?,
            links,
        );
        let composite_bytes = composite
            .to_dag_cbor()
            .map_err(|_| ClientUpdateError::InvalidGraph)?;
        let composite_signature = sign(&mut composite)?.ok_or(ClientUpdateError::InvalidGraph)?;
        let input =
            serde_json::to_value(doc.to_map().map_err(|_| ClientUpdateError::InvalidGraph)?)
                .map_err(|_| ClientUpdateError::InvalidGraph)?;
        Ok(Self {
            collection: schema.name.clone(),
            schema: Arc::new(schema.clone()),
            input,
            doc_id: doc_id.to_owned(),
            public_key_hex: signer.public_key_hex.clone(),
            waves: ClientWaves::new(
                children,
                child_signatures,
                composite_bytes,
                composite_signature,
            ),
        })
    }

    pub fn matches_input(&self, collection: &str, doc_id: &str, input: &serde_json::Value) -> bool {
        self.collection == collection
            && self.doc_id == doc_id
            && Self::normalize_input(&crdt::Crdts::default(), &self.schema, input)
                .and_then(|doc| doc.to_map().ok())
                .and_then(|map| serde_json::to_value(map).ok())
                .is_some_and(|input| self.input == input)
    }

    pub fn public_key_hex(&self) -> &str {
        &self.public_key_hex
    }
    pub fn doc_id(&self) -> &str {
        &self.doc_id
    }
    pub fn is_complete(&self) -> bool {
        self.waves.is_complete()
    }
    pub fn child_blocks(&self) -> &[Vec<u8>] {
        self.waves.children()
    }
    pub fn composite_block(&self) -> &[u8] {
        self.waves.composite()
    }

    /// Release only the complete expected wave, once.
    pub fn respond(&mut self, wave: &[Vec<u8>]) -> Result<Vec<Vec<u8>>, ClientUpdateError> {
        self.waves
            .respond(wave)
            .ok_or(ClientUpdateError::UnexpectedWave)
    }
}
