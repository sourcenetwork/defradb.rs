//! Delegations issued under every spelling vera.rs takes for an issuer.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

use super::*;
use crate::did_spelling;
use crate::signer::{Ed25519Signer, P256Signer, Secp256k1Signer};

const ISSUED_AT: u64 = 1_700_000_000;

/// A signer holding `inner`'s key, named by `did`.
struct Respelled {
    inner: Box<dyn DelegationSigner>,
    did: String,
}

impl DelegationSigner for Respelled {
    fn did(&self) -> &str {
        &self.did
    }

    fn algorithm(&self) -> DelegationAlgorithm {
        self.inner.algorithm()
    }

    fn sign<'a>(
        &'a self,
        signing_input: &'a [u8],
    ) -> defra_core::thread_bounds::MaybeBoxFuture<'a, Result<Vec<u8>, SignerError>> {
        self.inner.sign(signing_input)
    }
}

/// `did`, a base58btc `did:key`, with its multibase rewritten in base64url.
fn base64url(did: &str) -> String {
    let multibase = did.strip_prefix("did:key:z").expect("a base58btc did:key");
    let bytes = bs58::decode(multibase).into_vec().expect("base58btc");
    format!("did:key:u{}", URL_SAFE_NO_PAD.encode(bytes))
}

/// The four spellings vera.rs takes for one secp256k1 key.
fn secp256k1_spellings() -> Vec<String> {
    let key = k256::ecdsa::SigningKey::from_slice(&[7; 32]).expect("valid key");
    let compressed = did_from_signing_key(&key, true);
    let uncompressed = did_from_signing_key(&key, false);
    vec![
        base64url(&compressed),
        base64url(&uncompressed),
        compressed,
        uncompressed,
    ]
}

fn worker() -> String {
    did_from_signing_key(
        &k256::ecdsa::SigningKey::from_slice(&[9; 32]).expect("valid key"),
        true,
    )
}

async fn issue(signer: &dyn DelegationSigner, issuer: &str) -> Result<String, BearerError> {
    create_scoped_bearer_token_at(
        signer,
        issuer,
        &worker(),
        9001,
        300,
        DelegationScope::PolicyCommands,
        ISSUED_AT,
    )
    .await
}

#[nuclei::test]
#[mantra_macros::req_test("defra.acp.vera_issuer_spelling")]
async fn a_secp256k1_signer_issues_under_every_spelling_vera_rs_takes_and_keeps_it() {
    let spellings = secp256k1_spellings();
    for named in &spellings {
        let signer = Respelled {
            inner: Box::new(Secp256k1Signer::from_slice(&[7; 32]).expect("valid key")),
            did: named.clone(),
        };
        for issuer in &spellings {
            let token = issue(&signer, issuer)
                .await
                .unwrap_or_else(|error| panic!("{named} under {issuer}: {error}"));
            let claims = vera_crypto::jwt::verify_bearer_token(&token)
                .unwrap_or_else(|error| panic!("vera.rs refuses {issuer}: {error}"));
            assert_eq!(&claims.iss, issuer);
            assert!(did_spelling::same_actor(claims.actor(), named));
        }
    }
}

#[nuclei::test]
#[mantra_macros::req_test("defra.acp.vera_issuer_spelling")]
async fn an_ed25519_or_p256_signer_named_in_base64url_issues_under_its_canonical_did_only() {
    let signers: Vec<Box<dyn DelegationSigner>> = vec![
        Box::new(Ed25519Signer::from_slice(&[7; 32]).expect("valid key")),
        Box::new(P256Signer::from_slice(&[7; 32]).expect("valid key")),
    ];
    for inner in signers {
        let canonical = did_spelling::wire(inner.did()).into_owned();
        let signer = Respelled {
            did: base64url(&canonical),
            inner,
        };

        let token = issue(&signer, &did_spelling::wire(signer.did()))
            .await
            .expect("issued under the canonical did:key");
        let claims = vera_crypto::jwt::verify_bearer_token(&token).expect("vera.rs takes it");
        assert_eq!(claims.iss, canonical);

        // vera.rs takes an Ed25519 or P-256 key in base58btc only.
        assert!(vera_crypto::jwt::canonical_actor(signer.did()).is_err());
        assert!(matches!(
            issue(&signer, signer.did()).await,
            Err(BearerError::Issuer)
        ));
    }
}

#[nuclei::test]
#[mantra_macros::req_test("defra.acp.vera_issuer_spelling")]
async fn no_spelling_of_another_key_is_a_signers_issuer() {
    let signer = Secp256k1Signer::from_slice(&[8; 32]).expect("valid key");
    for issuer in secp256k1_spellings() {
        assert!(matches!(
            issue(&signer, &issuer).await,
            Err(BearerError::Issuer)
        ));
    }
}
