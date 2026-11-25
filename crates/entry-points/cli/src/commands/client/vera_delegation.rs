//! The caller's own Vera delegation, which a node whose document ACP is a
//! vera.rs deployment takes as the caller's bearer token.
//!
//! Such a node acts on Vera for a caller only under the caller's authority:
//! an identity token proves who is asking but lets the node submit nothing in
//! the caller's name. The node names, in its ACP status, the worker and
//! deployment a delegation must be issued for; the caller signs one with its
//! key and sends it in place of the identity token.

use std::time::Duration;

use vera_delegation::{DelegationSigner, Ed25519Signer, Secp256k1Signer};

use super::http_client::HttpClient;
use crate::error::{Error, Result};

/// How long a delegation holds: as long as an identity token does.
const LIFETIME: Duration = Duration::from_secs(15 * 60);

/// The delegation `key_bytes` signs for the node at `url`, or `None` when
/// the node names no delegation audience, which a node whose document ACP is
/// local or absent does not, and one that cannot be asked does not either.
pub(super) async fn for_node(
    url: &str,
    name: &str,
    key_bytes: &[u8],
    edge: &edge::Edge,
) -> Result<Option<String>> {
    let Ok(status) = HttpClient::new(url, edge)?.acp_status().await else {
        return Ok(None);
    };
    let Some(audience) = status.delegation else {
        return Ok(None);
    };
    let signer = signer(name, key_bytes)?;
    vera_delegation::caller_delegation(signer.as_ref(), &audience, LIFETIME)
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
    match key_bytes.len() {
        32 => Ok(Box::new(
            Secp256k1Signer::from_slice(key_bytes).map_err(invalid)?,
        )),
        64 => Ok(Box::new(
            Ed25519Signer::from_slice(key_bytes).map_err(invalid)?,
        )),
        len => Err(Error::InvalidIdentity(format!(
            "invalid key length for '{name}': {len} bytes (expected 32 for secp256k1 or 64 for \
             ed25519)"
        ))),
    }
}
