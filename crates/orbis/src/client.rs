//! Orbis signing through a registered key derivation.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use crypto::{Key, PublicKey};
use defra_core::signing::{RemoteSigner, SignFailure, SigningAuthorization};
use defra_core::thread_bounds::MaybeBoxFuture;
use edge::Edge;
use identity::{FullIdentity, Identity};
use sha2::{Digest, Sha256};

/// The gRPC channel to Orbis, on nuclei sockets.
type Channel = net::grpc::Channel<tonic::body::Body>;

use crate::proto::{sign_service_client::SignServiceClient, StartSignRequest};

#[derive(Debug, thiserror::Error)]
#[error("Orbis signing: {message}")]
pub struct OrbisClientError {
    message: String,
    /// The call did not reach Orbis or its answer did not come back.
    transient: bool,
}

/// Signs with a registered derivation and verifies against a provisioned BLS key.
///
/// The gRPC connection and every signing call run on the network runtime the
/// client is given.
pub struct OrbisClient {
    channel: Channel,
    derivation_id: String,
    public_key: crypto::BlsPublicKey,
    public_key_hex: String,
    signer_did: String,
    service_identity: Arc<identity::RawIdentity>,
    edge: Edge,
}

impl OrbisClient {
    /// The public key must be provisioned independently of the signing endpoint.
    pub async fn new(
        endpoint: String,
        derivation_id: String,
        public_key_bytes: Vec<u8>,
        service_identity: Arc<identity::RawIdentity>,
        edge: Edge,
    ) -> Result<Self, OrbisClientError> {
        if derivation_id.len() != 64
            || !derivation_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(error(
                "derivation ID must be 64 lowercase hexadecimal characters",
            ));
        }
        if service_identity.pub_key().key_type() != crypto::KeyType::Ed25519 {
            return Err(error("service identity must use Ed25519"));
        }
        if public_key_bytes.len() != 48 {
            return Err(error("BLS public key must contain 48 bytes"));
        }
        let public_key = crypto::BlsPublicKey::from_bytes(&public_key_bytes).map_err(error)?;
        let signer_did = public_key.did().map_err(error)?;
        let public_key_hex = hex::encode(public_key.raw());
        let endpoint = net::grpc::Endpoint::from_shared(endpoint, edge.clone())
            .map_err(error)?
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60));
        let channel = edge
            .spawn(async move { endpoint.connect().await })
            .await
            .map_err(error)?
            .map_err(error)?;
        Ok(Self {
            channel,
            derivation_id,
            public_key,
            public_key_hex,
            signer_did,
            service_identity,
            edge,
        })
    }

    pub fn signer_did(&self) -> &str {
        &self.signer_did
    }
    pub fn public_key_bytes(&self) -> &[u8] {
        self.public_key.raw()
    }
    pub fn public_key_hex(&self) -> &str {
        &self.public_key_hex
    }

    /// What one signing call needs, owned, so a wave's calls run as
    /// network tasks of their own.
    fn derivation(&self) -> Derivation {
        Derivation {
            channel: self.channel.clone(),
            derivation_id: self.derivation_id.clone(),
            public_key: self.public_key.clone(),
            service_identity: Arc::clone(&self.service_identity),
        }
    }
}

struct Derivation {
    channel: Channel,
    derivation_id: String,
    public_key: crypto::BlsPublicKey,
    service_identity: Arc<identity::RawIdentity>,
}

impl Derivation {
    fn bearer_token(&self, data: &[u8]) -> Result<String, OrbisClientError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(error)?
            .as_secs();
        let claims = serde_json::json!({
            "iss": self.service_identity.did().map_err(error)?.to_string(),
            "iat": now, "nbf": now, "exp": now.checked_add(300).ok_or_else(|| error("timestamp overflow"))?,
            "jti": uuid::Uuid::new_v4().to_string(),
            "derivation_id": self.derivation_id,
            "message_sha256": Sha256::digest(data).to_vec(),
        });
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"EdDSA","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).map_err(error)?);
        let message = format!("{header}.{payload}");
        let signature = self
            .service_identity
            .sign(message.as_bytes())
            .map_err(error)?;
        Ok(format!("{message}.{}", URL_SAFE_NO_PAD.encode(signature)))
    }

    async fn sign_async(&self, data: &[u8]) -> Result<Vec<u8>, OrbisClientError> {
        if data.len() > 1024 * 1024 {
            return Err(error("message exceeds Orbis's 1 MiB limit"));
        }
        let mut request = tonic::Request::new(StartSignRequest {
            message: data.to_vec(),
            derivation_id: self.derivation_id.clone(),
            valid_window: None,
        });
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", self.bearer_token(data)?)
                .parse()
                .map_err(error)?,
        );
        let response = SignServiceClient::new(self.channel.clone())
            .start_sign(request)
            .await
            .map_err(|status| OrbisClientError {
                transient: matches!(
                    status.code(),
                    tonic::Code::Unavailable
                        | tonic::Code::DeadlineExceeded
                        | tonic::Code::ResourceExhausted
                        | tonic::Code::Aborted
                ),
                message: status.to_string(),
            })?
            .into_inner();
        if response.signature.len() != 192 {
            return Err(error("BLS signature must contain 96 hex-encoded bytes"));
        }
        let signature = hex::decode(response.signature).map_err(error)?;
        if !self.public_key.verify(data, &signature).map_err(error)? {
            return Err(error("invalid signature"));
        }
        Ok(signature)
    }
}

/// Orbis binds authorization to the registered derivation.
const PER_REQUEST_AUTHORIZATION: &str =
    "Orbis authorization is bound to the registered derivation; per-request overrides are unsupported";

impl RemoteSigner for OrbisClient {
    fn sign_sync(
        &self,
        data: &[u8],
        authorization: Option<&SigningAuthorization>,
    ) -> Result<Vec<u8>, String> {
        if authorization.is_some() {
            return Err(PER_REQUEST_AUTHORIZATION.into());
        }
        self.edge
            .block_on(self.derivation().sign_async(data))
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }

    /// Every payload of the wave signs at once, each a call on the network
    /// runtime over the one gRPC channel; the wave completes when all have.
    fn sign_batch(
        &self,
        payloads: Vec<Arc<[u8]>>,
        authorization: Option<SigningAuthorization>,
    ) -> MaybeBoxFuture<'static, Vec<Result<Vec<u8>, SignFailure>>> {
        if authorization.is_some() {
            let refused = payloads
                .iter()
                .map(|_| Err(SignFailure::refused(PER_REQUEST_AUTHORIZATION)))
                .collect();
            return Box::pin(std::future::ready(refused));
        }
        let derivation = Arc::new(self.derivation());
        let calls: Vec<_> = payloads
            .into_iter()
            .map(|payload| {
                let derivation = Arc::clone(&derivation);
                self.edge
                    .spawn(async move { derivation.sign_async(&payload).await })
            })
            .collect();
        Box::pin(async move {
            let mut signatures = Vec::with_capacity(calls.len());
            for call in calls {
                signatures.push(match call.await {
                    Ok(Ok(signature)) => Ok(signature),
                    Ok(Err(error)) => Err(SignFailure {
                        transient: error.transient,
                        message: error.to_string(),
                    }),
                    Err(error) => Err(SignFailure::transient(error.to_string())),
                });
            }
            signatures
        })
    }
}

fn error(error: impl std::fmt::Display) -> OrbisClientError {
    OrbisClientError {
        message: error.to_string(),
        transient: false,
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
