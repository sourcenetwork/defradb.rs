//! The actors the tests write as.

use defra_node::{EmbeddedSigner, QueryRequest};
use identity::Did;
use vera::vera_rs::{DelegationSigner as _, Ed25519Signer};

/// An actor: an Ed25519 key, its DID, and the signer that puts its key on a
/// request.
#[derive(Clone)]
pub struct Actor {
    /// The seed and the public key, as DefraDB keeps an Ed25519 key.
    pub key: Vec<u8>,
    pub did: Did,
    pub signer: EmbeddedSigner,
}

impl Actor {
    pub fn new(seed: u8) -> Self {
        use crypto::Key as _;
        use identity::Identity as _;

        let seed = [seed; 32];
        let did = Ed25519Signer::from_slice(&seed)
            .expect("key")
            .did()
            .to_owned();
        let (_, public) = crypto::parse_did_key(&did).expect("did:key");
        let key = [seed.as_slice(), &public].concat();
        let private = crypto::Ed25519PrivateKey::from_bytes(&key).expect("key");
        let identity = identity::RawIdentity::from_ed25519(private.clone()).expect("identity");
        let did = identity.did().expect("did");
        let public = identity.public_key_bytes();
        let signer = EmbeddedSigner::from_config(defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: private.raw().to_vec(),
            public_key_hex: hex::encode(&public),
            public_key_bytes: public,
            remote_signer: None,
            signing_authorization: None,
        })
        .expect("signer");
        Self { key, did, signer }
    }

    /// `query` as this actor, signed with its key.
    pub fn signed(&self, query: impl Into<String>) -> QueryRequest {
        self.signer.attach(self.unsigned(query))
    }

    /// `query` as this actor, with no key of its own on it.
    pub fn unsigned(&self, query: impl Into<String>) -> QueryRequest {
        QueryRequest::new(query.into()).with_identity(Some(self.did.clone()))
    }
}
