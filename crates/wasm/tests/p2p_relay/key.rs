//! A secp256k1 identity generated for one test.

use std::time::Duration;

use crypto::keys::Key as _;
use identity::{Identity as _, IdentityKeyType, RawIdentity};

use crate::node::API;

pub struct Key {
    pub private_key_hex: String,
    pub public_key_hex: String,
    pub did: String,
    raw: RawIdentity,
}

impl Key {
    pub fn generate() -> Self {
        let private_key_hex = crypto::generate_secp256k1().unwrap().to_hex_string();
        let public_key_hex =
            crypto::private_key_from_string(crypto::KeyType::Secp256k1, &private_key_hex)
                .unwrap()
                .public_key()
                .to_hex_string();
        let raw = RawIdentity::from_identity_key_type(
            IdentityKeyType::Secp256k1,
            &hex::decode(&private_key_hex).unwrap(),
        )
        .unwrap();
        let did = raw.did().unwrap().to_string();
        Self {
            private_key_hex,
            public_key_hex,
            did,
            raw,
        }
    }

    /// A bearer token the node accepts: it checks the audience against the
    /// Host header, which is the API's authority.
    pub fn token(&self) -> String {
        let audience = API.trim_start_matches("http://").to_string();
        let token = identity::new_token(&self.raw, Duration::from_secs(3600), Some(audience), None)
            .unwrap();
        String::from_utf8(token).unwrap()
    }
}
