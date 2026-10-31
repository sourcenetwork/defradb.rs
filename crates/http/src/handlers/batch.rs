//! Batch signing HTTP handlers.
//!
//! These endpoints allow the Go indexer to drive batch CID collection
//! and signing over the HTTP API:
//!   POST /api/v0/batch/start  — begin collecting CIDs
//!   POST /api/v0/batch/sign   — sign collected CIDs, return BatchSignature
//!   POST /api/v0/batch/verify — verify a BatchSignature against CIDs

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::HttpError;
use crate::identity_extractor::ExtractIdentity;
use crate::query_context::resolve_signing_config;
use crate::router::AppState;

/// POST /api/v0/batch/start
///
/// Starts (or resets) batch CID collection for the caller's identity.
pub async fn batch_start(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<()>, HttpError> {
    let config = resolve_signing_config(&state, &identity)
        .ok_or_else(|| HttpError::BadRequest("no signing identity available".into()))?;

    defra_core::batch_signing::batch_start(&config.public_key_hex);

    Ok(Json(()))
}

/// Response from the batch sign endpoint.
#[derive(Serialize)]
pub struct BatchSignResponse {
    pub sig_type: String,
    pub identity: String,
    #[serde(with = "hex_serde")]
    pub value: Vec<u8>,
    #[serde(with = "hex_serde")]
    pub merkle_root: Vec<u8>,
    pub cid_count: usize,
}

/// POST /api/v0/batch/sign
///
/// Takes all collected CIDs for the caller's identity, computes a Merkle
/// root, signs it, and returns the batch signature as JSON.
pub async fn batch_sign(
    State(state): State<AppState>,
    identity: ExtractIdentity,
) -> Result<Json<BatchSignResponse>, HttpError> {
    let config = resolve_signing_config(&state, &identity)
        .ok_or_else(|| HttpError::BadRequest("no signing identity available".into()))?;

    let cids = defra_core::batch_signing::batch_take_cids(&config.public_key_hex)
        .ok_or_else(|| HttpError::BadRequest("no batch session active".into()))?;

    if cids.is_empty() {
        return Err(HttpError::BadRequest("batch has no CIDs".into()));
    }

    let sig = crypto::batch::sign_batch(&cids, &config)
        .map_err(|e| HttpError::Internal(format!("batch sign failed: {}", e)))?;

    Ok(Json(BatchSignResponse {
        sig_type: sig.sig_type,
        identity: sig.identity,
        value: sig.value,
        merkle_root: sig.merkle_root,
        cid_count: sig.cid_count,
    }))
}

/// Request body for the batch verify endpoint.
#[derive(Deserialize)]
pub struct BatchVerifyRequest {
    pub sig_type: String,
    pub identity: String,
    #[serde(with = "hex_serde")]
    pub value: Vec<u8>,
    #[serde(with = "hex_serde")]
    pub merkle_root: Vec<u8>,
    pub cid_count: usize,
    pub cids: Vec<String>,
    /// The Merkle tree to check against: Go Shinzo's when absent, or
    /// `RustLegacy` for a signature made before Go's became the default.
    #[serde(default)]
    pub merkle_scheme: defra_core::batch_signing::MerkleScheme,
}

/// POST /api/v0/batch/verify
///
/// Verifies a batch signature against the provided CID list.
pub async fn batch_verify(Json(req): Json<BatchVerifyRequest>) -> Result<Json<()>, HttpError> {
    let cids: Vec<cid::Cid> = req
        .cids
        .iter()
        .map(|s| s.parse::<cid::Cid>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| HttpError::BadRequest(format!("invalid CID: {}", e)))?;

    let sig = crypto::batch::BatchSignature {
        sig_type: req.sig_type,
        identity: req.identity,
        value: req.value,
        merkle_root: req.merkle_root,
        cid_count: req.cid_count,
        merkle_scheme: req.merkle_scheme,
        batch_scope: Default::default(),
    };
    let options = defra_core::batch_signing::BatchOptions {
        scheme: req.merkle_scheme,
        ..Default::default()
    };

    let valid = crypto::batch::verify_batch_signature_with(&sig, &cids, options)
        .map_err(|e| HttpError::Internal(format!("batch verify failed: {}", e)))?;

    if valid {
        Ok(Json(()))
    } else {
        Err(HttpError::BadRequest(
            "batch signature verification failed".into(),
        ))
    }
}

mod hex_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    use super::*;

    /// A batch signature the base revision of #2454 made over "doc0".."doc2"
    /// with Rust's old Merkle tree; it carries no scheme.
    fn old_signature(merkle_scheme: Option<&str>) -> BatchVerifyRequest {
        let mut body = serde_json::json!({
            "sig_type": "EdDSA",
            "identity": "fabd7aa83c1055a21d6c176138011a96035dbc2ef1af10708067e8af2b15a427",
            "value": "ba768beb92db641e431301c51ee83661e4dc7c2d90ca9e456817bac810407cc30ad69f506adfef8aa5b401522ec76b6b6de1cb2e7a3c6646e18359cb72b8c300",
            "merkle_root": "fc8f2a4d773c6db591bdb6b6e122a3fb44274049c8d7d4b0b68212a1dcc8bfc9",
            "cid_count": 3,
            "cids": [
                "bafyreihzcm257sh3jalv3gf5z4ifkgonns5qhgc644twfyc2izce24zomu",
                "bafyreiggh264xyama235smkdavv6fozx27w4o7cqo3a52oy4tey7g3o3ri",
                "bafyreidys6rnehmxq7uqr6emxwunoiibai62tbtilhwiqj6yartrrdauhy",
            ],
        });
        if let Some(scheme) = merkle_scheme {
            body["merkle_scheme"] = scheme.into();
        }
        serde_json::from_value(body).unwrap()
    }

    /// The handler does no I/O, so one poll finishes it.
    fn verify(request: BatchVerifyRequest) -> Result<Json<()>, HttpError> {
        let mut future = pin!(batch_verify(Json(request)));
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("batch verify waited on nothing"),
        }
    }

    #[test]
    fn an_old_signature_verifies_only_when_the_old_tree_is_named() {
        assert!(
            verify(old_signature(None)).is_err(),
            "Go's tree is the default"
        );
        assert!(verify(old_signature(Some("GoShinzo"))).is_err());
        assert!(verify(old_signature(Some("RustLegacy"))).is_ok());
    }
}
