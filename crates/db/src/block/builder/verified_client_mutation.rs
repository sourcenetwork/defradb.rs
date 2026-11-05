use std::sync::Arc;

use crypto::Key as _;
use defra_core::signing::{SigningConfig, SigningKeyType};
use rapidhash::{HashMapExt, RapidHashMap};
use schema::CollectionVersion;

use super::{
    client_waves::ClientWaves, verified_client_create::ExactSignatures, ClientDeleteError,
    ClientDeleteSignatures, ClientUpdateError, ClientUpdateSignatures, ClientUpdateSnapshot,
};

/// Public evidence for the exact signed waves, without a client private key.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientMutationEvidence {
    public_key_hex: String,
    doc_id: String,
    child_blocks: Vec<Vec<u8>>,
    child_signatures: Vec<Vec<u8>>,
    composite_block: Vec<u8>,
    composite_signature: Vec<u8>,
}

impl ClientMutationEvidence {
    pub(super) fn from_waves(key: &str, doc_id: &str, waves: &ClientWaves) -> Self {
        Self {
            public_key_hex: key.to_owned(),
            doc_id: doc_id.to_owned(),
            child_blocks: waves.children().to_vec(),
            child_signatures: waves.child_signatures().to_vec(),
            composite_block: waves.composite().to_vec(),
            composite_signature: waves.composite_signature().to_vec(),
        }
    }

    fn signing_config(&self) -> Result<(String, SigningConfig), VerifiedMutationError> {
        if self.child_blocks.len() != self.child_signatures.len() || self.composite_block.is_empty()
        {
            return Err(VerifiedMutationError::InvalidEvidence);
        }
        let total = self
            .child_blocks
            .iter()
            .chain(&self.child_signatures)
            .chain([&self.composite_block, &self.composite_signature])
            .try_fold(0usize, |total, bytes| total.checked_add(bytes.len()))
            .ok_or(VerifiedMutationError::InvalidEvidence)?;
        if total > 1024 * 1024 {
            return Err(VerifiedMutationError::InvalidEvidence);
        }
        let public = crypto::public_key_from_string(crypto::KeyType::Ed25519, &self.public_key_hex)
            .map_err(|_| VerifiedMutationError::InvalidEvidence)?;
        let did = public
            .did()
            .map_err(|_| VerifiedMutationError::InvalidEvidence)?;
        let mut signatures = RapidHashMap::new();
        for (block, signature) in
            self.child_blocks
                .iter()
                .zip(&self.child_signatures)
                .chain(std::iter::once((
                    &self.composite_block,
                    &self.composite_signature,
                )))
        {
            if !public
                .verify(block, signature)
                .map_err(|_| VerifiedMutationError::InvalidEvidence)?
                || signatures
                    .insert(block.clone(), signature.clone())
                    .is_some()
            {
                return Err(VerifiedMutationError::InvalidEvidence);
            }
        }
        Ok((
            did,
            SigningConfig {
                key_type: SigningKeyType::Ed25519,
                private_key_bytes: Vec::new(),
                public_key_bytes: public.raw_owned(),
                public_key_hex: self.public_key_hex.clone(),
                remote_signer: Some(Arc::new(ExactSignatures(signatures))),
                signing_authorization: None,
            },
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VerifiedMutationError {
    #[error("invalid or unsigned mutation evidence")]
    InvalidEvidence,
    #[error(transparent)]
    Update(#[from] ClientUpdateError),
    #[error(transparent)]
    Delete(#[from] ClientDeleteError),
}

enum Graph {
    Update(ClientUpdateSignatures),
    Delete(ClientDeleteSignatures),
}

/// Verified evidence is not an ACP grant; callers must execute with its signer DID.
pub struct VerifiedClientMutation {
    graph: Graph,
    signer_did: String,
    signing_config: SigningConfig,
}

impl VerifiedClientMutation {
    pub fn verify_update(
        crdts: &crdt::Crdts,
        schema: &CollectionVersion,
        doc_id: &str,
        input: &serde_json::Value,
        snapshot: &ClientUpdateSnapshot,
        evidence: ClientMutationEvidence,
    ) -> Result<Self, VerifiedMutationError> {
        let (signer_did, signing_config) = evidence.signing_config()?;
        let graph =
            ClientUpdateSignatures::build(crdts, schema, doc_id, input, snapshot, &signing_config)?;
        if graph.evidence() != evidence {
            return Err(VerifiedMutationError::InvalidEvidence);
        }
        Ok(Self {
            graph: Graph::Update(graph),
            signer_did,
            signing_config,
        })
    }

    pub fn verify_delete(
        schema: &CollectionVersion,
        doc_id: &str,
        snapshot: &ClientUpdateSnapshot,
        evidence: ClientMutationEvidence,
    ) -> Result<Self, VerifiedMutationError> {
        let (signer_did, signing_config) = evidence.signing_config()?;
        let graph = ClientDeleteSignatures::build(schema, doc_id, snapshot, &signing_config)?;
        if graph.evidence() != evidence {
            return Err(VerifiedMutationError::InvalidEvidence);
        }
        Ok(Self {
            graph: Graph::Delete(graph),
            signer_did,
            signing_config,
        })
    }

    pub fn signer_did(&self) -> &str {
        &self.signer_did
    }
    pub fn doc_id(&self) -> &str {
        match &self.graph {
            Graph::Update(g) => g.doc_id(),
            Graph::Delete(g) => g.doc_id(),
        }
    }
    pub fn matches_mutation(&self, mutation: &query::mapper::Mutation) -> bool {
        if mutation.encryption().is_some()
            || !query::mapper::valid_unaliased_doc_id_selection(&mutation.fields)
        {
            return false;
        }
        match &self.graph {
            Graph::Update(g) => {
                mutation.mutation_type == query::MutationType::Update
                    && query::mapper::single_document_mutation_target(mutation, false)
                        == Some(g.doc_id())
                    && serde_json::to_value(&mutation.update_input)
                        .ok()
                        .is_some_and(|input| {
                            g.matches_input(&mutation.collection_name, g.doc_id(), &input)
                        })
            }
            Graph::Delete(g) => {
                mutation.mutation_type == query::MutationType::Delete
                    && query::mapper::single_document_mutation_target(mutation, true)
                        == Some(g.doc_id())
                    && g.matches_target(&mutation.collection_name, g.doc_id())
            }
        }
    }
    /// The signer releases signatures only for the verified graph's blocks.
    #[doc(hidden)]
    pub fn signing_config(&self) -> SigningConfig {
        self.signing_config.clone()
    }
}
