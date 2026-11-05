use std::sync::Arc;

use defra_core::signing::{RemoteSigner, SigningAuthorization, SigningConfig, SigningKeyType};
use rapidhash::{HashMapExt, RapidHashMap};
use schema::CollectionVersion;

use super::{ClientCreateError, ClientCreateSignatures};

/// Public signature evidence, never a client private key or an ACP grant.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientCreateEvidence {
    pub(super) public_key_hex: String,
    pub(super) doc_id: String,
    pub(super) child_blocks: Vec<Vec<u8>>,
    pub(super) child_signatures: Vec<Vec<u8>>,
    pub(super) composite_block: Vec<u8>,
    pub(super) composite_signature: Vec<u8>,
}

impl std::fmt::Debug for ClientCreateEvidence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClientCreateEvidence")
            .field("child_count", &self.child_blocks.len())
            .finish_non_exhaustive()
    }
}

struct ExactSignatures(RapidHashMap<Vec<u8>, Vec<u8>>);
impl RemoteSigner for ExactSignatures {
    fn sign_sync(&self, data: &[u8], _: Option<&SigningAuthorization>) -> Result<Vec<u8>, String> {
        self.0
            .get(data)
            .cloned()
            .ok_or_else(|| "block is outside the approved create graph".to_owned())
    }
}

/// An exact create graph verified against a caller-supplied trusted schema and input.
/// It does not authenticate the caller or authorize any document operation.
pub struct VerifiedClientCreate {
    graph: ClientCreateSignatures,
    signer_did: String,
    signing_config: SigningConfig,
}

impl VerifiedClientCreate {
    pub fn verify(
        crdts: &crdt::Crdts,
        schema: &CollectionVersion,
        input: &serde_json::Value,
        evidence: ClientCreateEvidence,
    ) -> Result<Self, ClientCreateError> {
        if evidence.child_blocks.len() != evidence.child_signatures.len() {
            return Err(ClientCreateError::InvalidGraph);
        }
        let total = evidence
            .child_blocks
            .iter()
            .chain(&evidence.child_signatures)
            .chain([&evidence.composite_block, &evidence.composite_signature])
            .try_fold(0usize, |total, bytes| total.checked_add(bytes.len()))
            .ok_or(ClientCreateError::InvalidGraph)?;
        if total > 1024 * 1024 {
            return Err(ClientCreateError::InvalidGraph);
        }
        let public =
            crypto::public_key_from_string(crypto::KeyType::Ed25519, &evidence.public_key_hex)
                .map_err(|_| ClientCreateError::InvalidGraph)?;
        let signer_did = public.did().map_err(|_| ClientCreateError::InvalidGraph)?;
        let mut signatures = RapidHashMap::new();
        for (block, signature) in evidence
            .child_blocks
            .iter()
            .zip(&evidence.child_signatures)
            .chain(std::iter::once((
                &evidence.composite_block,
                &evidence.composite_signature,
            )))
        {
            if !public
                .verify(block, signature)
                .map_err(|_| ClientCreateError::InvalidGraph)?
                || signatures
                    .insert(block.clone(), signature.clone())
                    .is_some()
            {
                return Err(ClientCreateError::InvalidGraph);
            }
        }
        let signing_config = SigningConfig {
            key_type: SigningKeyType::Ed25519,
            private_key_bytes: Vec::new(),
            public_key_bytes: public.raw_owned(),
            public_key_hex: evidence.public_key_hex.clone(),
            remote_signer: Some(Arc::new(ExactSignatures(signatures))),
            signing_authorization: None,
        };
        let doc = ClientCreateSignatures::normalize_input(schema, input)
            .ok_or(ClientCreateError::UnsupportedDocument)?;
        let graph = ClientCreateSignatures::build(crdts, &doc, &signing_config)?;
        let recomputed = graph.evidence();
        if graph.doc_id() != evidence.doc_id
            || recomputed.child_blocks.len() != evidence.child_blocks.len()
            || recomputed.composite_block != evidence.composite_block
            || recomputed.composite_signature != evidence.composite_signature
        {
            return Err(ClientCreateError::InvalidGraph);
        }
        Ok(Self {
            graph,
            signer_did,
            signing_config,
        })
    }

    pub fn signer_did(&self) -> &str {
        &self.signer_did
    }
    pub fn doc_id(&self) -> &str {
        self.graph.doc_id()
    }
    pub fn matches_input(&self, collection: &str, input: &serde_json::Value) -> bool {
        self.graph.matches_input(collection, input)
    }
    pub fn matches_mutation(&self, mutation: &query::mapper::Mutation) -> bool {
        mutation.mutation_type == query::MutationType::Add
            && mutation.create_input.len() == 1
            && mutation.encryption().is_none()
            && query::mapper::valid_unaliased_doc_id_selection(&mutation.fields)
            && serde_json::to_value(&mutation.create_input[0])
                .ok()
                .is_some_and(|input| self.matches_input(&mutation.collection_name, &input))
    }
    /// The returned signer can sign only the exact blocks in this verified graph.
    #[doc(hidden)]
    pub fn signing_config(&self) -> SigningConfig {
        self.signing_config.clone()
    }
}
