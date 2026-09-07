//! The key this browser authors with. It signs the blocks the tab writes and
//! mints the token that authenticates them, and it never leaves the tab.
//!
//! Without one, blocks go out unsigned: `/sync` derives ownership from the
//! verified genesis signer, so an unsigned document is owned by nobody and its
//! author can never grant access to it.

use std::time::Duration;

use defra_core::signing::{SigningConfig, SigningKeyType};
use identity::{Identity, IdentityKeyType, RawIdentity};
use zeroize::Zeroize;

use crate::error::{Result, WasmError};

/// How long a minted token stays valid. Long enough to outlive a page's sync
/// session, short enough that a leaked one expires.
const TOKEN_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// The `d` of an RFC 8037 Ed25519 JWK, which is the seed alone.
const ED25519_SEED_BYTES: usize = 32;

pub(crate) struct ClientIdentity {
    raw: RawIdentity,
    did: String,
    signing: SigningConfig,
}

impl ClientIdentity {
    pub(crate) fn from_private_key(private_key_hex: &str, key_type: &str) -> Result<Self> {
        let (key_type, signing_key_type) = parse_key_type(key_type)?;
        let mut bytes = hex::decode(private_key_hex.trim_start_matches("0x"))
            .map_err(|error| WasmError::Identity(format!("private key is not hex: {error}")))?;
        // A browser generating its key with WebCrypto exports an RFC 8037 JWK,
        // whose `d` is the 32-byte seed, while an Ed25519 key here is the
        // 64-byte seed || public form. Promote at this boundary, as the CLI's
        // JWK import does, rather than in the shared constructor.
        if key_type == IdentityKeyType::Ed25519 && bytes.len() == ED25519_SEED_BYTES {
            let promoted = crypto::ed25519_key_from_seed(&bytes)
                .map_err(|error| WasmError::Identity(error.to_string()))?;
            bytes.zeroize();
            bytes = promoted;
        }
        let raw = RawIdentity::from_identity_key_type(key_type, &bytes)
            .map_err(|error| WasmError::Identity(error.to_string()))?;
        bytes.zeroize();
        let did = raw
            .did()
            .map_err(|error| WasmError::Identity(error.to_string()))?
            .to_string();

        let public_key_bytes = raw.public_key_bytes();
        let signing = SigningConfig {
            key_type: signing_key_type,
            private_key_bytes: SigningConfig::private_key_bytes_from_vec(raw.private_key_bytes()),
            public_key_hex: hex::encode(&public_key_bytes),
            public_key_bytes,
            remote_signer: None,
            signing_authorization: None,
        };

        Ok(Self { raw, did, signing })
    }

    pub(crate) fn did(&self) -> &str {
        &self.did
    }

    /// How this client appears in a signature header: Go's
    /// `PublicKey().String()`, the hex-encoded key as bytes.
    pub(crate) fn signer_identity(&self) -> Vec<u8> {
        self.signing.public_key_hex.as_bytes().to_vec()
    }

    pub(crate) fn signing_config(&self) -> SigningConfig {
        self.signing.clone()
    }

    /// A self-signed JWT naming this identity. The node verifies it against the
    /// request's Host header, so the audience is the server being synced with.
    pub(crate) fn auth_token(&self, audience: Option<String>) -> Result<String> {
        let token = identity::new_token(&self.raw, TOKEN_TTL, audience, None)
            .map_err(|error| WasmError::Identity(error.to_string()))?;
        String::from_utf8(token)
            .map_err(|error| WasmError::Identity(format!("token is not valid UTF-8: {error}")))
    }
}

fn parse_key_type(key_type: &str) -> Result<(IdentityKeyType, SigningKeyType)> {
    match key_type
        .to_ascii_lowercase()
        .replace(['-', '_'], "")
        .as_str()
    {
        "ed25519" => Ok((IdentityKeyType::Ed25519, SigningKeyType::Ed25519)),
        "secp256k1" => Ok((IdentityKeyType::Secp256k1, SigningKeyType::Secp256k1)),
        // Accepting it would mint tokens and then fail every write: block
        // signing refuses secp256r1 without a remote signer, since a Secure
        // Enclave key cannot be exported, and a browser has no signer to reach.
        "secp256r1" => Err(WasmError::Identity(
            "secp256r1 cannot sign in a browser: it needs a remote signer".into(),
        )),
        other => Err(WasmError::Identity(format!(
            "unsupported key type '{other}': expected ed25519 or secp256k1"
        ))),
    }
}

/// Installs a signing config for as long as it is held.
///
/// The config is a thread-local the write path reads, and blocks arriving from
/// the server must not be signed with this key, so it is installed around a
/// local write and taken away again.
///
/// One writer at a time: the caller holds the mutation lock, because a guard
/// dropped while another mutation is still in flight would leave that one
/// writing unsigned blocks.
pub(crate) struct SigningGuard;

impl SigningGuard {
    pub(crate) fn install(identity: Option<&ClientIdentity>) -> Self {
        defra_core::signing::set_signing_config(identity.map(ClientIdentity::signing_config));
        Self
    }
}

impl Drop for SigningGuard {
    fn drop(&mut self) {
        defra_core::signing::set_signing_config(None);
    }
}
