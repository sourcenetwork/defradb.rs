use defra_core::block::{Block, CompositeDeltaPayload, CrdtDelta, Signature};
use defra_core::signing::{SigningConfig, SigningKeyType};
use schema::CollectionVersion;

use super::{client_waves::ClientWaves, compute_signature, ClientUpdateSnapshot};

#[derive(Debug, thiserror::Error)]
pub enum ClientDeleteError {
    #[error(
        "client signing requires a live unencrypted document of the trusted nonbranchable schema"
    )]
    UnsupportedSnapshot,
    #[error("client signing requires a local Ed25519 key")]
    UnsupportedSigner,
    #[error("client delete graph could not be built: {0}")]
    Build(String),
    #[error("client delete graph contains an invalid signature or block")]
    InvalidGraph,
    #[error("signing wave does not match the client's approved delete graph")]
    UnexpectedWave,
}

/// A signature prepared for one exact native deletion of a verified document.
pub struct ClientDeleteSignatures {
    collection: String,
    doc_id: String,
    public_key_hex: String,
    waves: ClientWaves,
}

impl ClientDeleteSignatures {
    pub fn from_snapshot(
        schema: &CollectionVersion,
        doc_id: &str,
        snapshot: &ClientUpdateSnapshot,
        signer: &SigningConfig,
    ) -> Result<Self, ClientDeleteError> {
        if signer.key_type != SigningKeyType::Ed25519
            || signer.remote_signer.is_some()
            || signer.private_key_bytes.is_empty()
        {
            return Err(ClientDeleteError::UnsupportedSigner);
        }
        if snapshot.doc_id != doc_id
            || snapshot.versions.len() != 1
            || !snapshot.versions.contains(&schema.version_id)
            || schema.is_branchable
        {
            return Err(ClientDeleteError::UnsupportedSnapshot);
        }
        let heads = snapshot
            .heads
            .get("C")
            .cloned()
            .ok_or(ClientDeleteError::UnsupportedSnapshot)?;
        let priority = snapshot
            .max_priority
            .checked_add(1)
            .ok_or(ClientDeleteError::UnsupportedSnapshot)?;
        let block = Block::new(
            CrdtDelta::Composite(CompositeDeltaPayload {
                collection_version_id: schema.version_id.clone(),
                priority,
                status: 2,
            }),
            heads,
            vec![],
        );
        let bytes = block
            .to_dag_cbor()
            .map_err(|_| ClientDeleteError::InvalidGraph)?;
        let (_, encoded) = compute_signature(&block, signer)
            .map_err(ClientDeleteError::Build)?
            .ok_or(ClientDeleteError::InvalidGraph)?;
        let signature =
            Signature::from_dag_cbor(&encoded).map_err(|_| ClientDeleteError::InvalidGraph)?;
        let public =
            crypto::public_key_from_string(crypto::KeyType::Ed25519, &signer.public_key_hex)
                .map_err(|_| ClientDeleteError::InvalidGraph)?;
        if signature.header.identity != signer.public_key_hex.as_bytes()
            || !public
                .verify(&bytes, &signature.value)
                .map_err(|_| ClientDeleteError::InvalidGraph)?
        {
            return Err(ClientDeleteError::InvalidGraph);
        }
        Ok(Self {
            collection: schema.name.clone(),
            doc_id: doc_id.to_owned(),
            public_key_hex: signer.public_key_hex.clone(),
            waves: ClientWaves::new(vec![], vec![], bytes, signature.value),
        })
    }

    pub fn matches_target(&self, collection: &str, doc_id: &str) -> bool {
        self.collection == collection && self.doc_id == doc_id
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
    pub fn composite_block(&self) -> &[u8] {
        self.waves.composite()
    }
    pub fn respond(&mut self, wave: &[Vec<u8>]) -> Result<Vec<Vec<u8>>, ClientDeleteError> {
        self.waves
            .respond(wave)
            .ok_or(ClientDeleteError::UnexpectedWave)
    }
}
