//! Orbis signing through a registered key derivation.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use crypto::{Key, PublicKey};
use defra_core::signing::{RemoteSigner, SigningAuthorization};
use identity::{FullIdentity, Identity};
use sha2::{Digest, Sha256};
use tonic::transport::Channel;

use crate::proto::{StartSignRequest, sign_service_client::SignServiceClient};

#[derive(Debug, thiserror::Error)]
#[error("Orbis signing: {0}")]
pub struct OrbisClientError(String);

/// Signs with a registered derivation and verifies against a provisioned BLS key.
pub struct OrbisClient {
    channel: Channel,
    derivation_id: String,
    public_key: crypto::BlsPublicKey,
    public_key_hex: String,
    signer_did: String,
    service_identity: Arc<identity::RawIdentity>,
    runtime: Option<tokio::runtime::Runtime>,
}

impl OrbisClient {
    /// The public key must be provisioned independently of the signing endpoint.
    pub async fn new(
        endpoint: String,
        derivation_id: String,
        public_key_bytes: Vec<u8>,
        service_identity: Arc<identity::RawIdentity>,
    ) -> Result<Self, OrbisClientError> {
        if derivation_id.len() != 64
            || !derivation_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(OrbisClientError(
                "derivation ID must be 64 lowercase hexadecimal characters".into(),
            ));
        }
        if service_identity.pub_key().key_type() != crypto::KeyType::Ed25519 {
            return Err(OrbisClientError("service identity must use Ed25519".into()));
        }
        if public_key_bytes.len() != 48 {
            return Err(error("BLS public key must contain 48 bytes"));
        }
        let public_key = crypto::BlsPublicKey::from_bytes(&public_key_bytes).map_err(error)?;
        let signer_did = public_key.did().map_err(error)?;
        let public_key_hex = hex::encode(public_key.raw());
        let channel = Channel::from_shared(endpoint)
            .map_err(error)?
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .connect()
            .await
            .map_err(error)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(error)?;
        Ok(Self {
            channel,
            derivation_id,
            public_key,
            public_key_hex,
            signer_did,
            service_identity,
            runtime: Some(runtime),
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
            .map_err(error)?
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

impl RemoteSigner for OrbisClient {
    fn sign_sync(
        &self,
        data: &[u8],
        authorization: Option<&SigningAuthorization>,
    ) -> Result<Vec<u8>, String> {
        if authorization.is_some() {
            return Err("Orbis authorization is bound to the registered derivation; per-request overrides are unsupported".into());
        }
        let runtime = self.runtime.as_ref().ok_or("Orbis runtime is stopped")?;
        if tokio::runtime::Handle::try_current().is_ok() {
            std::thread::scope(|scope| {
                scope
                    .spawn(|| runtime.block_on(self.sign_async(data)))
                    .join()
            })
            .map_err(|_| "Orbis signing worker panicked".to_owned())?
            .map_err(|e| e.to_string())
        } else {
            runtime
                .block_on(self.sign_async(data))
                .map_err(|e| e.to_string())
        }
    }
}

impl Drop for OrbisClient {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

fn error(error: impl std::fmt::Display) -> OrbisClientError {
    OrbisClientError(error.to_string())
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
