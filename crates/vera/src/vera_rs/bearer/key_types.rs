//! Delegations from each key type, checked by vera.rs's own verifier.

use std::borrow::Cow;

use vera_client::DelegationScope;

use super::*;
use crate::vera_rs::did_spelling;
use crate::vera_rs::signer::{Ed25519Signer, P256Signer, Secp256k1Signer};

const ISSUED_AT: u64 = 1_700_000_000;

fn signers() -> Vec<(&'static str, Box<dyn DelegationSigner>)> {
    vec![
        (
            "ES256K",
            Box::new(Secp256k1Signer::from_slice(&[7; 32]).expect("valid key")),
        ),
        (
            "EdDSA",
            Box::new(Ed25519Signer::from_slice(&[7; 32]).expect("valid key")),
        ),
        (
            "ES256",
            Box::new(P256Signer::from_slice(&[7; 32]).expect("valid key")),
        ),
    ]
}

fn worker() -> String {
    did_from_signing_key(
        &vera_client::Secp256k1SigningKey::from_slice(&[9; 32]).expect("valid key"),
        true,
    )
}

#[nuclei::test]
async fn vera_rs_verifies_a_delegation_from_each_key_type_and_names_its_actor() {
    for (algorithm, signer) in signers() {
        assert_eq!(signer.algorithm().name(), algorithm);
        let issuer = did_spelling::wire(signer.did());
        let token = issue_at(
            signer.as_ref(),
            &issuer,
            &worker(),
            9001,
            300,
            DelegationScope::PolicyCommands,
            ISSUED_AT,
        )
        .await
        .expect("token");

        let claims = vera_crypto::jwt::verify_bearer_token(&token)
            .unwrap_or_else(|error| panic!("vera.rs refuses the {algorithm} token: {error}"));
        // vera.rs names the actor in its own spelling; it is the same actor.
        assert!(
            did_spelling::same_actor(claims.actor(), signer.did()),
            "{algorithm}: {} is not {}",
            claims.actor(),
            signer.did()
        );
    }
}

#[nuclei::test]
async fn a_p256_delegation_is_issued_under_the_compressed_spelling_only() {
    let signer = P256Signer::from_slice(&[7; 32]).expect("valid key");
    // DefraDB's spelling of the key is not one vera.rs takes for an issuer.
    assert!(matches!(did_spelling::wire(signer.did()), Cow::Owned(_)));
    let refused = issue_at(
        &signer,
        signer.did(),
        &worker(),
        9001,
        300,
        DelegationScope::PolicyCommands,
        ISSUED_AT,
    )
    .await;
    assert!(matches!(refused, Err(BearerError::Issuer)));
}

#[nuclei::test]
async fn a_signer_whose_algorithm_and_key_disagree_issues_nothing() {
    struct Mislabelled(Ed25519Signer);

    impl DelegationSigner for Mislabelled {
        fn did(&self) -> &str {
            self.0.did()
        }

        fn algorithm(&self) -> DelegationAlgorithm {
            DelegationAlgorithm::Es256
        }

        fn sign<'a>(
            &'a self,
            signing_input: &'a [u8],
        ) -> defra_core::thread_bounds::MaybeBoxFuture<'a, Result<Vec<u8>, SignerError>> {
            self.0.sign(signing_input)
        }
    }

    let signer = Mislabelled(Ed25519Signer::from_slice(&[7; 32]).expect("valid key"));
    let refused = issue_at(
        &signer,
        signer.did(),
        &worker(),
        9001,
        300,
        DelegationScope::PolicyCommands,
        ISSUED_AT,
    )
    .await;
    assert!(matches!(
        refused,
        Err(BearerError::Signer(SignerError::Did { .. }))
    ));
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
