use defra_core::signing::{SigningConfig, SigningKeyType};
use identity::{Identity as _, IdentityKeyType, RawIdentity};
use query::QueryRequest;

/// A local signing capability deliberately supplied by an embedded caller.
///
/// The caller is responsible for possessing and authorizing the key; a DID or
/// membership in the process-wide identity registry is not a capability. The
/// request actor must match this key, and native ACP still authorizes the actor.
#[derive(Clone)]
pub struct EmbeddedSigner {
    did: String,
    config: SigningConfig,
}

/// Why a local signing capability cannot be built or cannot sign a request.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddedSignerError {
    #[error("embedded signing requires a local Ed25519 key without a remote signer")]
    UnsupportedSigner,
    #[error("invalid Ed25519 private key")]
    InvalidPrivateKey,
    #[error("private and public signing keys do not match")]
    PublicKeyMismatch,
    #[error("cannot derive a DID from the signing key")]
    InvalidDid,
    #[error("explicit embedded signing does not allow DAC bypass")]
    DacBypass,
    #[error("explicit signing key must match the selected actor")]
    ActorMismatch,
}

impl EmbeddedSigner {
    /// Capture and validate an existing local key without looking it up by DID.
    pub fn from_config(config: SigningConfig) -> Result<Self, EmbeddedSignerError> {
        if config.key_type != SigningKeyType::Ed25519
            || !config.has_local_private_key()
            || config.has_remote_signer()
            || config.signing_authorization.is_some()
        {
            return Err(EmbeddedSignerError::UnsupportedSigner);
        }
        let identity = RawIdentity::from_identity_key_type(
            IdentityKeyType::Ed25519,
            &config.private_key_bytes,
        )
        .map_err(|_| EmbeddedSignerError::InvalidPrivateKey)?;
        let public_key = identity.public_key_bytes();
        if public_key != config.public_key_bytes || hex::encode(public_key) != config.public_key_hex
        {
            return Err(EmbeddedSignerError::PublicKeyMismatch);
        }
        let did = identity
            .did()
            .map_err(|_| EmbeddedSignerError::InvalidDid)?
            .to_string();
        Ok(Self { did, config })
    }

    /// The identity derived from this capability's verified keypair.
    pub fn did(&self) -> &str {
        &self.did
    }

    /// `request`, signed with this key instead of the node's when
    /// [`Node::execute_request`](crate::Node::execute_request) or
    /// [`Node::execute_request_in_txn`](crate::Node::execute_request_in_txn)
    /// runs it.
    pub fn attach(&self, mut request: QueryRequest) -> QueryRequest {
        request.context = request.context.with_signing(Some(self.config.clone()));
        request
    }
}

/// The key a caller put on `request`, once it may sign it: a usable local key
/// of the actor the caller named, with DAC left on.
pub(crate) fn admitted_signer(
    request: &QueryRequest,
) -> Result<Option<SigningConfig>, EmbeddedSignerError> {
    let Some(config) = request.context.signing() else {
        return Ok(None);
    };
    if request.context.dac_bypass() {
        return Err(EmbeddedSignerError::DacBypass);
    }
    let signer = EmbeddedSigner::from_config(config.clone())?;
    if request
        .identity
        .as_ref()
        .map(ToString::to_string)
        .as_deref()
        != Some(signer.did())
    {
        return Err(EmbeddedSignerError::ActorMismatch);
    }
    Ok(Some(signer.config))
}
