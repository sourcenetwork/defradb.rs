//! A registered identity's delegation, checked by vera.rs's own verifier.

use super::*;
use crate::vera_rs::did_spelling;

fn worker() -> String {
    did_from_signing_key(
        &vera_client::Secp256k1SigningKey::from_slice(&[9; 32]).expect("valid key"),
        true,
    )
}

#[nuclei::test]
async fn a_registered_ed25519_or_p256_identity_gets_a_delegation_vera_rs_verifies() {
    use defra_core::signing::{SigningConfig, SigningKeyType};
    use identity::Identity as _;

    use crate::vera_rs::provider_commands::resolve_registered_or_passthrough_bearer_token;

    let identities = [
        (
            SigningKeyType::Ed25519,
            identity::RawIdentity::from_ed25519(crypto::generate_ed25519().expect("key"))
                .expect("identity"),
        ),
        (
            SigningKeyType::Secp256r1,
            identity::RawIdentity::from_secp256r1(crypto::generate_secp256r1().expect("key"))
                .expect("identity"),
        ),
    ];
    for (key_type, identity) in identities {
        let did = identity.did().expect("did").to_string();
        defra_core::signing::store_identity(
            &did,
            SigningConfig {
                key_type,
                private_key_bytes: SigningConfig::private_key_bytes_from_vec(
                    identity.private_key_bytes().to_vec(),
                ),
                public_key_bytes: identity.public_key_bytes().to_vec(),
                public_key_hex: hex::encode(identity.public_key_bytes()),
                remote_signer: None,
                signing_authorization: None,
            },
        )
        .expect("the store takes the identity");

        let token = resolve_registered_or_passthrough_bearer_token(&did, &worker(), 9001)
            .await
            .expect("resolution")
            .expect("a token built from the registered key");
        let claims = vera_crypto::jwt::verify_bearer_token(&token)
            .unwrap_or_else(|error| panic!("vera.rs refuses the {key_type:?} token: {error}"));
        assert!(did_spelling::same_actor(claims.actor(), &did));
    }
}
