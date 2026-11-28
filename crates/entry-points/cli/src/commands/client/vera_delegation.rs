//! The caller's own Vera delegation, which a node whose document ACP is a
//! vera.rs deployment takes as the caller's bearer token.
//!
//! Such a node acts on Vera for a caller only under the caller's authority:
//! an identity token proves who is asking but lets the node submit nothing in
//! the caller's name. The node names, in its ACP status, the worker and
//! deployment a delegation must be issued for; the caller signs one with its
//! key and sends it in place of the identity token.

use vera_delegation::{DelegationSigner, Ed25519Signer, Secp256k1Signer};

use super::http_client::HttpClient;
use super::{
    generate_auth_token_from_key_bytes, key_type_from_identity_bytes, AUTH_TOKEN_LIFETIME,
};
use crate::error::{Error, Result};

/// The delegation `key_bytes` signs for the node at `url`, or `None` when
/// the node names no delegation audience, which a node whose document ACP is
/// local or absent does not. A node that cannot be asked falls back to the
/// identity token, as before.
pub(super) async fn for_node(
    url: &str,
    name: &str,
    key_bytes: &[u8],
    edge: &edge::Edge,
) -> Result<Option<String>> {
    let identity_token = generate_auth_token_from_key_bytes(name, key_bytes, url, None)?;
    let Ok(status) = HttpClient::new(url, edge)?
        .with_auth_token(Some(identity_token))
        .acp_status()
        .await
    else {
        return Ok(None);
    };
    let Some(audience) = status.delegation else {
        return Ok(None);
    };
    let signer = signer(name, key_bytes)?;
    vera_delegation::caller_delegation(signer.as_ref(), &audience, AUTH_TOKEN_LIFETIME)
        .await
        .map(Some)
        .map_err(|e| Error::InvalidIdentity(format!("Vera delegation for '{name}': {e}")))
}

/// The signer of a secp256k1 key (32 bytes) or an Ed25519 key (its seed and
/// public key, 64 bytes), as DefraDB keeps identities.
fn signer(name: &str, key_bytes: &[u8]) -> Result<Box<dyn DelegationSigner>> {
    let invalid = |e: vera_delegation::SignerError| {
        Error::InvalidIdentity(format!("invalid key for '{name}': {e}"))
    };
    match key_type_from_identity_bytes(name, key_bytes)? {
        crypto::KeyType::Secp256k1 => Ok(Box::new(
            Secp256k1Signer::from_slice(key_bytes).map_err(invalid)?,
        )),
        crypto::KeyType::Ed25519 => Ok(Box::new(
            Ed25519Signer::from_slice(key_bytes).map_err(invalid)?,
        )),
        key_type => Err(Error::InvalidIdentity(format!(
            "unsupported delegation key type '{key_type}' for '{name}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::Key;
    use vera_delegation::DelegationAlgorithm;

    #[test]
    fn an_ed25519_identity_signs_an_eddsa_delegation() {
        let key = crypto::generate_ed25519().unwrap();
        let key_bytes = hex::decode(key.to_hex_string()).unwrap();

        assert_eq!(key_bytes.len(), 64);
        assert_eq!(
            signer("Ed25519 identity", &key_bytes).unwrap().algorithm(),
            DelegationAlgorithm::EdDsa
        );
    }
}
