//! Block signing configuration for CRDT block signing.
//!
//! Provides the SigningConfig type, which a request's
//! [`RequestContext`](crate::RequestContext) carries from the boundary to the
//! block builder layer.

use std::fmt;
use std::sync::Arc;

use kovan_map::HopscotchMap;
use regolith::sync::OnceCell;

use crate::thread_bounds::MaybeBoxFuture;
use rapidhash::fast::RandomState;
use zeroize::Zeroize;

mod identity_store;

pub use identity_store::{
    clear_identity_store, find_remote_signer_did, get_identity, hold_identity, release_identity,
    retain_identity, store_identity, IdentityConflict, IdentityHold,
};

#[cfg(any(test, feature = "ambient"))]
pub use crate::ambient::{
    get_broadcast_creator_did, get_request_bearer_token, get_signing_config,
    scope_broadcast_creator_did, scope_request_bearer_token, set_broadcast_creator_did,
    set_signing_config,
};

/// Remote signing delegate (e.g. Orbis, Secure Enclave host callbacks).
///
/// Implementations call an external service to produce signatures.
/// `sign_sync` must be callable from synchronous code: the block builder
/// computes and signs a block without awaiting. [`sign_batch`](Self::sign_batch) is the
/// completion form the write path uses: one call per wave of signatures,
/// completing without holding a thread while the service works.
pub trait RemoteSigner: Send + Sync {
    fn sign_sync(
        &self,
        data: &[u8],
        authorization: Option<&SigningAuthorization>,
    ) -> Result<Vec<u8>, String>;

    /// Signs every payload of a wave, one result per payload in order, and
    /// completes when all are signed. A signer that reaches a service
    /// overrides it to run the payloads concurrently and hold no thread; the
    /// default signs each with [`sign_sync`](Self::sign_sync) before it
    /// returns, as a signer with only a synchronous call must.
    fn sign_batch(
        &self,
        payloads: Vec<Arc<[u8]>>,
        authorization: Option<SigningAuthorization>,
    ) -> MaybeBoxFuture<'static, Vec<Result<Vec<u8>, SignFailure>>> {
        let signatures = payloads
            .iter()
            .map(|payload| {
                self.sign_sync(payload, authorization.as_ref())
                    .map_err(SignFailure::refused)
            })
            .collect();
        Box::pin(std::future::ready(signatures))
    }
}

/// A signature a signer could not make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignFailure {
    /// What went wrong, for the caller's error.
    pub message: String,
    /// The failure was on the way to the signer (a transport error, a
    /// timeout): signing the same bytes again may succeed and is safe.
    pub transient: bool,
}

impl SignFailure {
    /// The signer refused or failed in a way asking again does not fix.
    pub fn refused(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            transient: false,
        }
    }

    /// The request did not reach the signer or its answer did not come back.
    pub fn transient(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            transient: true,
        }
    }
}

impl fmt::Display for SignFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Optional authorization context passed to remote signers.
///
/// For Orbis-backed signing, this lets DefraDB attach ACP metadata to a sign
/// request so Orbis can authorize the signature ceremony before producing a
/// threshold signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum SigningAuthorization {
    Policy {
        policy_id: String,
        resource: String,
        object_id: String,
        permission: String,
    },
    Decision {
        decision_id: String,
    },
}

/// Exact delegated-signing input needed by a transaction attempt. `data` is
/// the canonical unsigned block encoding; the remaining fields bind the
/// response to the signer identity and authorization context.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RemoteSignatureInput {
    pub data: Vec<u8>,
    pub key_type: SigningKeyType,
    pub public_key_hex: String,
    pub authorization: Option<SigningAuthorization>,
}

impl fmt::Debug for RemoteSignatureInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteSignatureInput")
            .field("data", &"[redacted]")
            .field("key_type", &self.key_type)
            .field("signer_context", &"[redacted]")
            .finish()
    }
}

impl RemoteSignatureInput {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|error| format!("encode remote signature input: {error}"))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes)
            .map_err(|error| format!("decode remote signature input: {error}"))
    }

    pub fn resolve(&self, signer: &SigningConfig) -> Result<Vec<u8>, String> {
        self.remote_signer(signer)?
            .sign_sync(&self.data, self.authorization.as_ref())
    }

    /// The remote signer of `signer` that may answer this input: the one it
    /// was prepared for, with the same key and authorization.
    pub fn remote_signer(&self, signer: &SigningConfig) -> Result<Arc<dyn RemoteSigner>, String> {
        if self.key_type != signer.key_type
            || self.public_key_hex != signer.public_key_hex
            || self.authorization != signer.signing_authorization
        {
            return Err(
                "prepared signature context no longer matches the configured signer".into(),
            );
        }
        signer
            .remote_signer
            .clone()
            .ok_or_else(|| "prepared remote signature requires a remote signer".to_string())
    }
}

#[cfg(test)]
mod remote_signature_input_tests {
    use super::*;

    #[test]
    fn remote_signature_request_round_trips_exact_bytes_and_context() {
        let input = RemoteSignatureInput {
            data: vec![0x81, 0x01, 0x02],
            key_type: SigningKeyType::BlsAugV1,
            public_key_hex: "public-key-id".to_string(),
            authorization: Some(SigningAuthorization::Decision {
                decision_id: "decision-7".to_string(),
            }),
        };
        let encoded = input.encode().unwrap();
        assert_eq!(RemoteSignatureInput::decode(&encoded).unwrap(), input);

        let changed_head_data = RemoteSignatureInput {
            data: vec![0x81, 0x01, 0x03],
            ..input.clone()
        };
        assert_ne!(input.encode().unwrap(), changed_head_data.encode().unwrap());
    }
}

/// Key type for signing operations.
///
/// Replaces raw string matching with a type-safe enum. Serializes to/from
/// lowercase strings ("ed25519", "secp256k1", "secp256r1", "bls_aug_v1") for
/// backward compatibility with JSON/FFI boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum SigningKeyType {
    Ed25519,
    Secp256k1,
    Secp256r1,
    #[serde(rename = "bls_aug_v1")]
    BlsAugV1,
}

impl SigningKeyType {
    /// Return the string representation for backward compatibility.
    ///
    /// Callers that previously used `.key_type.as_str()` on the old String
    /// field can use this method without changes.
    pub fn as_str(&self) -> &'static str {
        match self {
            SigningKeyType::Ed25519 => "ed25519",
            SigningKeyType::Secp256k1 => "secp256k1",
            SigningKeyType::Secp256r1 => "secp256r1",
            SigningKeyType::BlsAugV1 => "bls_aug_v1",
        }
    }

    /// Infallible conversion to the block-level `SignatureType`.
    pub fn to_signature_type(self) -> crate::block::SignatureType {
        match self {
            SigningKeyType::Ed25519 => crate::block::SignatureType::EdDSA,
            SigningKeyType::Secp256k1 => crate::block::SignatureType::ES256K,
            SigningKeyType::Secp256r1 => crate::block::SignatureType::ES256,
            SigningKeyType::BlsAugV1 => crate::block::SignatureType::BLSAugV1,
        }
    }
}

impl fmt::Display for SigningKeyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SigningKeyType::Ed25519 => write!(f, "ed25519"),
            SigningKeyType::Secp256k1 => write!(f, "secp256k1"),
            SigningKeyType::Secp256r1 => write!(f, "secp256r1"),
            SigningKeyType::BlsAugV1 => write!(f, "bls_aug_v1"),
        }
    }
}

impl std::str::FromStr for SigningKeyType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ed25519" => Ok(SigningKeyType::Ed25519),
            "secp256k1" => Ok(SigningKeyType::Secp256k1),
            "secp256r1" => Ok(SigningKeyType::Secp256r1),
            "bls_aug_v1" => Ok(SigningKeyType::BlsAugV1),
            other => Err(format!("unsupported signing key type: {other}")),
        }
    }
}

/// Backward-compat: allow `key_type == "secp256k1"` comparisons.
impl PartialEq<&str> for SigningKeyType {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// Backward-compat: allow converting to String for callers that need it.
impl From<SigningKeyType> for String {
    fn from(kt: SigningKeyType) -> Self {
        kt.to_string()
    }
}

impl From<SigningKeyType> for crate::block::SignatureType {
    fn from(kt: SigningKeyType) -> Self {
        kt.to_signature_type()
    }
}

/// Signing configuration containing key material for block signing.
pub struct SigningConfig {
    /// Signing key type.
    pub key_type: SigningKeyType,
    /// Raw private key bytes (empty for remote signers like Orbis)
    pub private_key_bytes: Vec<u8>,
    /// Raw public key bytes (for identity in signature header)
    pub public_key_bytes: Vec<u8>,
    /// Public key hex string (for signature header identity, matches Go's pubKey.String())
    pub public_key_hex: String,
    /// Optional remote signer for delegated signing (e.g. Orbis ring)
    pub remote_signer: Option<Arc<dyn RemoteSigner>>,
    /// Optional per-request authorization for delegated signing.
    pub signing_authorization: Option<SigningAuthorization>,
}

impl Clone for SigningConfig {
    fn clone(&self) -> Self {
        Self {
            key_type: self.key_type,
            private_key_bytes: Self::private_key_bytes_from_slice(&self.private_key_bytes),
            public_key_bytes: self.public_key_bytes.clone(),
            public_key_hex: self.public_key_hex.clone(),
            remote_signer: self.remote_signer.clone(),
            signing_authorization: self.signing_authorization.clone(),
        }
    }
}

impl Drop for SigningConfig {
    fn drop(&mut self) {
        self.private_key_bytes.zeroize();
    }
}

impl SigningConfig {
    /// Copy private key bytes into an exactly sized allocation.
    pub fn private_key_bytes_from_slice(bytes: &[u8]) -> Vec<u8> {
        let mut exact = Vec::with_capacity(bytes.len());
        exact.extend_from_slice(bytes);
        exact
    }

    /// Normalize owned private key bytes to an exactly sized allocation.
    pub fn private_key_bytes_from_vec(mut bytes: Vec<u8>) -> Vec<u8> {
        if bytes.len() == bytes.capacity() {
            bytes
        } else {
            let exact = Self::private_key_bytes_from_slice(&bytes);
            // Wipe spare capacity too: oversized buffers may have held key
            // material while being built incrementally.
            bytes.resize(bytes.capacity(), 0);
            bytes.zeroize();
            exact
        }
    }

    /// Returns true if this identity has locally exportable private key bytes.
    pub fn has_local_private_key(&self) -> bool {
        !self.private_key_bytes.is_empty()
    }

    /// Returns true if this identity can sign by delegating to a remote signer.
    pub fn has_remote_signer(&self) -> bool {
        self.remote_signer.is_some()
    }
}

/// Resolve signing config for a request identity when signing is enabled.
///
/// When an explicit identity DID is provided, uses that identity's signing config
/// (falling back to node identity for JWT-authenticated users without local keys).
///
/// Callers that need to respect a runtime signing flag should use
/// [`resolve_signing_config_with_flag`].
pub fn resolve_signing_config(
    identity_did: Option<&str>,
    node_identity_did: Option<&str>,
) -> Option<SigningConfig> {
    resolve_signing_config_with_flag(identity_did, node_identity_did, true)
}

/// Resolve signing config for a request identity.
///
/// When signing is enabled and an explicit identity DID is provided, uses that
/// identity's signing config (falling back to node identity for JWT-authenticated
/// users without local keys).
///
/// When no identity is provided (anonymous request):
/// - If signing is enabled on the node, uses the node identity to sign
///   (matching Go's `!signingDisabled` check)
/// - Otherwise, produces unsigned blocks
pub fn resolve_signing_config_with_flag(
    identity_did: Option<&str>,
    node_identity_did: Option<&str>,
    signing_enabled: bool,
) -> Option<SigningConfig> {
    if !signing_enabled {
        return None;
    }

    match identity_did {
        Some(did) if !did.is_empty() => {
            get_identity(did).or_else(|| node_identity_did.and_then(get_identity))
        }
        _ => {
            if signing_enabled {
                node_identity_did.and_then(get_identity)
            } else {
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Request Bearer Token — keyed by DID for ACP registration passthrough
// ---------------------------------------------------------------------------

/// The bearer tokens bound to DIDs for requests that carry none of their own
/// (see [`RequestContext::bearer_token_for`](crate::RequestContext::bearer_token_for)).
/// A request authenticated over HTTP carries its own token, so concurrent
/// requests for the same DID never replace each other's JWT.
fn request_token_store() -> &'static HopscotchMap<String, String, RandomState> {
    static STORE: OnceCell<HopscotchMap<String, String, RandomState>> = OnceCell::new();
    STORE.get_or_init_racy(|| HopscotchMap::with_hasher(RandomState::default()))
}

/// Bind `token` to `did` for requests that carry no bearer of their own.
///
/// When a user authenticates via JWT, the node doesn't have their private key
/// and can't create new bearer tokens for them. Instead, we pass through the
/// original JWT (which IS signed by the user's key) to vera.rs/Vera
/// for ACP operations like register_object.
pub fn set_request_bearer_token(did: &str, token: impl Into<String>) {
    request_token_store().insert(did.to_string(), token.into());
}

/// The bearer token bound to `did` for requests that carry none of their own.
pub fn stored_bearer_token(did: &str) -> Option<String> {
    request_token_store().get(did)
}

/// Remove the stored request bearer token for a specific DID.
pub fn clear_request_bearer_token(did: &str) {
    request_token_store().remove(did);
}

#[cfg(test)]
mod tests {
    use super::*;
    use regolith::sync::{Mutex, MutexGuard};

    #[test]
    fn request_bearer_scope_isolated_across_same_did_and_restores_global_fallback() {
        workers::block_on(async {
            let did = "did:key:bearer-scope-test";
            let other_did = "did:key:bearer-scope-other";
            set_request_bearer_token(did, "legacy-global");
            set_request_bearer_token(other_did, "other-global");

            let first = scope_request_bearer_token(
                Some(did.to_string()),
                Some("request-one".to_string()),
                async {
                    assert_eq!(
                        get_request_bearer_token(did).as_deref(),
                        Some("request-one")
                    );
                    workers::yield_now().await;
                    assert_eq!(
                        get_request_bearer_token(did).as_deref(),
                        Some("request-one")
                    );
                },
            );
            let second = scope_request_bearer_token(
                Some(did.to_string()),
                Some("request-two".to_string()),
                async {
                    assert_eq!(
                        get_request_bearer_token(did).as_deref(),
                        Some("request-two")
                    );
                    workers::yield_now().await;
                    assert_eq!(
                        get_request_bearer_token(did).as_deref(),
                        Some("request-two")
                    );
                },
            );
            futures::join!(first, second);

            scope_request_bearer_token(Some(did.to_string()), None, async {
                assert_eq!(get_request_bearer_token(did), None);
                assert_eq!(get_request_bearer_token(other_did), None);
            })
            .await;

            assert_eq!(
                get_request_bearer_token(did).as_deref(),
                Some("legacy-global")
            );
            clear_request_bearer_token(did);
            clear_request_bearer_token(other_did);
        })
        .unwrap()
    }

    fn make_config(label: &str) -> SigningConfig {
        SigningConfig {
            key_type: SigningKeyType::Secp256k1,
            private_key_bytes: vec![],
            public_key_bytes: vec![],
            public_key_hex: label.to_string(),
            remote_signer: None,
            signing_authorization: None,
        }
    }

    /// `IDENTITY_STORE` is process-global, so the tests that clear and refill it
    /// cannot run concurrently: one test's `store_identity` lands between
    /// another's `clear_identity_store` and its assertion.
    static IDENTITY_STORE_GUARD: Mutex<()> = Mutex::new(());

    async fn lock_identity_store() -> MutexGuard<'static, ()> {
        IDENTITY_STORE_GUARD.lock().await
    }

    #[test]
    fn resolve_signing_config_uses_request_identity_when_registered() {
        workers::block_on(async {
            let _guard = lock_identity_store().await;
            clear_identity_store();
            store_identity("did:key:request", make_config("request"))
                .expect("the store takes the identity");
            store_identity("did:key:node", make_config("node"))
                .expect("the store takes the identity");

            let disabled = resolve_signing_config_with_flag(
                Some("did:key:request"),
                Some("did:key:node"),
                false,
            );
            assert!(disabled.is_none());

            let resolved = resolve_signing_config(Some("did:key:request"), Some("did:key:node"))
                .expect("request identity should resolve");
            assert_eq!(resolved.public_key_hex, "request");

            clear_identity_store();
        })
        .unwrap()
    }

    #[test]
    fn resolve_signing_config_falls_back_to_node_identity_when_request_identity_missing() {
        workers::block_on(async {
            let _guard = lock_identity_store().await;
            clear_identity_store();
            store_identity("did:key:node", make_config("node"))
                .expect("the store takes the identity");

            let resolved = resolve_signing_config(Some("did:key:request"), Some("did:key:node"))
                .expect("node fallback should resolve");
            assert_eq!(resolved.public_key_hex, "node");

            clear_identity_store();
        })
        .unwrap()
    }

    #[test]
    fn find_remote_signer_did_returns_registered_remote_signer() {
        workers::block_on(async {
            let _guard = lock_identity_store().await;
            struct NoopRemoteSigner;

            impl RemoteSigner for NoopRemoteSigner {
                fn sign_sync(
                    &self,
                    _data: &[u8],
                    _authorization: Option<&SigningAuthorization>,
                ) -> Result<Vec<u8>, String> {
                    Ok(vec![])
                }
            }

            clear_identity_store();
            store_identity("did:key:local", make_config("local"))
                .expect("the store takes the identity");
            store_identity(
                "did:key:remote",
                SigningConfig {
                    key_type: SigningKeyType::BlsAugV1,
                    private_key_bytes: vec![],
                    public_key_bytes: vec![],
                    public_key_hex: "remote".to_string(),
                    remote_signer: Some(Arc::new(NoopRemoteSigner)),
                    signing_authorization: None,
                },
            )
            .expect("the store takes the identity");

            let resolved = find_remote_signer_did().expect("remote signer DID should resolve");
            assert_eq!(resolved, "did:key:remote");

            clear_identity_store();
        })
        .unwrap()
    }

    #[test]
    fn the_last_hold_on_an_identity_removes_its_key() {
        workers::block_on(async {
            let _guard = lock_identity_store().await;
            clear_identity_store();
            let did = "did:key:held";
            hold_identity(did, make_config("held")).expect("the store takes the identity");
            hold_identity(did, make_config("held")).expect("the store takes the identity");

            release_identity(did);
            assert!(get_identity(did).is_some(), "one hold remains");
            release_identity(did);
            assert!(get_identity(did).is_none(), "no hold remains");
            assert!(
                !retain_identity(did),
                "a released identity has no key to retain"
            );

            clear_identity_store();
        })
        .unwrap()
    }

    #[test]
    fn a_retained_identity_outlives_the_hold_it_was_taken_from() {
        workers::block_on(async {
            let _guard = lock_identity_store().await;
            clear_identity_store();
            let did = "did:key:retained";
            hold_identity(did, make_config("held")).expect("the store takes the identity");
            assert!(retain_identity(did));

            release_identity(did);
            assert!(get_identity(did).is_some());
            release_identity(did);
            assert!(get_identity(did).is_none());

            clear_identity_store();
        })
        .unwrap()
    }

    #[test]
    fn dropping_an_identity_hold_gives_it_back() {
        workers::block_on(async {
            let _guard = lock_identity_store().await;
            clear_identity_store();
            let did = "did:key:guarded";
            let hold = IdentityHold::new(did, make_config("guarded"));
            assert!(get_identity(did).is_some());

            drop(hold);
            assert!(get_identity(did).is_none());

            clear_identity_store();
        })
        .unwrap()
    }

    #[test]
    fn a_pinned_identity_survives_its_holds() {
        workers::block_on(async {
            let _guard = lock_identity_store().await;
            clear_identity_store();
            let did = "did:key:pinned";
            store_identity(did, make_config("pinned")).expect("the store takes the identity");
            assert!(retain_identity(did));
            release_identity(did);
            release_identity(did);
            assert!(get_identity(did).is_some());

            clear_identity_store();
        })
        .unwrap()
    }

    /// `proofs/tla/IdentityStore.tla`: a clear unpins and drops what nothing
    /// holds, but keeps a held key and its count, so a hold taken after the
    /// clear and one taken before it are both given back to the same slot.
    #[test]
    fn a_clear_keeps_held_keys_and_their_count() {
        workers::block_on(async {
            let _guard = lock_identity_store().await;
            clear_identity_store();
            let held = "did:key:held-through-clear";
            let pinned = "did:key:pinned-then-cleared";
            hold_identity(held, make_config("held")).expect("the store takes the identity");
            store_identity(pinned, make_config("pinned")).expect("the store takes the identity");

            clear_identity_store();
            assert!(get_identity(held).is_some(), "a held key survives a clear");
            assert!(get_identity(pinned).is_none(), "a clear unpins");

            hold_identity(held, make_config("held")).expect("the store takes the identity");
            release_identity(held);
            assert!(
                get_identity(held).is_some(),
                "the hold taken before the clear still counts"
            );
            release_identity(held);
            assert!(get_identity(held).is_none(), "no hold remains");

            clear_identity_store();
        })
        .unwrap()
    }

    /// A host signer: each registration wraps its own host context.
    struct HostSigner(#[allow(dead_code)] u8);

    impl RemoteSigner for HostSigner {
        fn sign_sync(
            &self,
            _data: &[u8],
            _authorization: Option<&SigningAuthorization>,
        ) -> Result<Vec<u8>, String> {
            Ok(vec![])
        }
    }

    fn remote_config(signer: &Arc<HostSigner>) -> SigningConfig {
        let mut config = make_config("remote");
        config.remote_signer = Some(signer.clone());
        config
    }

    fn signs_through(did: &str, signer: &Arc<HostSigner>) -> bool {
        get_identity(did).is_some_and(|config| {
            config
                .remote_signer
                .as_ref()
                .is_some_and(|stored| std::ptr::addr_eq(Arc::as_ptr(stored), Arc::as_ptr(signer)))
        })
    }

    /// `proofs/tla/IdentityStore.tla` INV_OwnSigner: while a DID is held, a
    /// hold that brings another signer is refused and takes nothing, so the
    /// first holder keeps signing through its own signer, also after the
    /// refused registration's owner gives its context up. Once the DID is
    /// free, another signer may take it.
    #[test]
    fn a_held_did_refuses_another_signer() {
        workers::block_on(async {
            let _guard = lock_identity_store().await;
            clear_identity_store();
            let did = "did:key:registered-twice";
            let first = Arc::new(HostSigner(1));
            let second = Arc::new(HostSigner(2));

            hold_identity(did, remote_config(&first)).expect("the first signer takes the DID");
            let refused = hold_identity(did, remote_config(&second))
                .expect_err("a second signer for a held DID is refused");
            assert_eq!(refused.did, did);
            assert!(signs_through(did, &first), "the holder keeps its signer");

            release_identity(did);
            assert!(get_identity(did).is_none(), "the refused hold took nothing");

            hold_identity(did, remote_config(&second)).expect("a free DID takes any signer");
            assert!(signs_through(did, &second));
            release_identity(did);

            clear_identity_store();
        })
        .unwrap()
    }

    /// INV_OwnSigner for pins: a pin may replace a signer only pinned, never
    /// one something holds.
    #[test]
    fn a_pin_replaces_only_an_unheld_signer() {
        workers::block_on(async {
            let _guard = lock_identity_store().await;
            clear_identity_store();
            let did = "did:key:pinned-and-held";
            let first = Arc::new(HostSigner(1));
            let second = Arc::new(HostSigner(2));

            store_identity(did, remote_config(&first)).expect("a free DID takes a pin");
            store_identity(did, remote_config(&second)).expect("a pin replaces a pin");
            assert!(signs_through(did, &second));

            assert!(retain_identity(did));
            store_identity(did, remote_config(&first))
                .expect_err("a pin may not re-route a holder");
            assert!(signs_through(did, &second), "the holder keeps its signer");
            store_identity(did, remote_config(&second)).expect("the same signer may pin");
            release_identity(did);

            clear_identity_store();
        })
        .unwrap()
    }

    #[test]
    fn private_key_bytes_from_vec_shrinks_spare_capacity() {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(&[1_u8, 2, 3, 4]);
        assert_eq!(bytes.capacity(), 64);

        let exact = SigningConfig::private_key_bytes_from_vec(bytes);

        assert_eq!(exact, vec![1, 2, 3, 4]);
        assert_eq!(exact.len(), exact.capacity());
    }

    #[test]
    fn private_key_bytes_from_slice_uses_exact_capacity() {
        let bytes = SigningConfig::private_key_bytes_from_slice(&[1_u8, 2, 3, 4]);

        assert_eq!(bytes, vec![1, 2, 3, 4]);
        assert_eq!(bytes.len(), bytes.capacity());
    }

    #[test]
    fn private_key_bytes_from_vec_reuses_exact_capacity() {
        let bytes = vec![1_u8, 2, 3, 4];
        assert_eq!(bytes.len(), bytes.capacity());

        let exact = SigningConfig::private_key_bytes_from_vec(bytes);

        assert_eq!(exact, vec![1, 2, 3, 4]);
        assert_eq!(exact.len(), exact.capacity());
    }

    #[test]
    fn signing_key_type_from_str_rejects_invalid_values() {
        let error = "invalid".parse::<SigningKeyType>().unwrap_err();
        assert_eq!(error, "unsupported signing key type: invalid");
    }
}

#[cfg(test)]
mod broadcast_creator_tests {
    use super::*;

    #[test]
    fn async_creator_scopes_are_isolated_from_each_other_and_thread_state() {
        workers::block_on(async {
            set_broadcast_creator_did(Some("did:key:stale-thread".to_string()));

            let alice = scope_broadcast_creator_did(Some("did:key:alice".to_string()), async {
                workers::yield_now().await;
                get_broadcast_creator_did()
            });
            let anonymous = scope_broadcast_creator_did(None, async {
                workers::yield_now().await;
                get_broadcast_creator_did()
            });

            let (alice_creator, anonymous_creator) = futures::join!(alice, anonymous);
            assert_eq!(alice_creator.as_deref(), Some("did:key:alice"));
            assert_eq!(anonymous_creator, None);

            set_broadcast_creator_did(None);
        })
        .unwrap()
    }

    #[test]
    fn completed_creator_scope_cannot_contaminate_next_anonymous_mutation() {
        workers::block_on(async {
            let owner = scope_broadcast_creator_did(Some("did:key:owner".to_string()), async {
                get_broadcast_creator_did()
            })
            .await;
            assert_eq!(owner.as_deref(), Some("did:key:owner"));

            let anonymous =
                scope_broadcast_creator_did(None, async { get_broadcast_creator_did() }).await;
            assert_eq!(anonymous, None);
        })
        .unwrap()
    }
}
