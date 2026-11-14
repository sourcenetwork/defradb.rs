use std::time::{SystemTime, UNIX_EPOCH};

use k256::ecdsa::VerifyingKey;
use vera_client::DelegationScope;

use super::signer::{admission, DelegationAlgorithm, DelegationSigner, SignerError};

/// A delegation `signer` gives `subject` for vera.rs ACP policy commands.
pub(super) async fn create_bearer_token(
    signer: &dyn DelegationSigner,
    issuer: &str,
    subject: &str,
    deployment_id: u64,
    expiry_secs: u64,
) -> Result<String, BearerError> {
    create_scoped_bearer_token(
        signer,
        issuer,
        subject,
        deployment_id,
        expiry_secs,
        DelegationScope::PolicyCommands,
    )
    .await
}

/// A delegation for one scope, issued under the spelling `issuer` gives.
///
/// vera.rs takes either spelling of a secp256k1 issuer as the same actor, and
/// an Ed25519 or P-256 issuer under its one canonical DID.
pub(super) async fn create_scoped_bearer_token(
    signer: &dyn DelegationSigner,
    issuer: &str,
    subject: &str,
    deployment_id: u64,
    expiry_secs: u64,
    scope: DelegationScope,
) -> Result<String, BearerError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| BearerError::Time(e.to_string()))?
        .as_secs();
    issue_at(
        signer,
        issuer,
        subject,
        deployment_id,
        expiry_secs,
        scope,
        now,
    )
    .await
}

/// The delegation as it is issued at `now`, in seconds since the epoch.
async fn issue_at(
    signer: &dyn DelegationSigner,
    issuer: &str,
    subject: &str,
    deployment_id: u64,
    expiry_secs: u64,
    scope: DelegationScope,
    now: u64,
) -> Result<String, BearerError> {
    let key = admission::actor_key(signer)?;
    if !key.names(issuer) {
        return Err(BearerError::Issuer);
    }

    let expiry = now
        .checked_add(expiry_secs)
        .filter(|expiry| *expiry > now)
        .ok_or_else(|| BearerError::Time("invalid delegation lifetime".into()))?;
    let header = base64url_encode(header(signer.algorithm()));
    let claims = Claims {
        aud: format!("vera:{deployment_id}"),
        exp: expiry,
        iat: now,
        iss: issuer,
        nbf: now.saturating_sub(30),
        scope,
        sub: subject,
    };
    let payload_json =
        serde_json::to_string(&claims).map_err(|e| BearerError::Claims(e.to_string()))?;
    let payload = base64url_encode(payload_json.as_bytes());

    let message = format!("{}.{}", header, payload);
    let signature = signer.sign(message.as_bytes()).await?;
    admission::admit(signer, &key, message.as_bytes(), &signature)?;
    let sig_b64 = base64url_encode(&signature);

    Ok(format!("{}.{}", message, sig_b64))
}

/// The claims of a delegation, in the order they are written into the token.
///
/// A struct fixes that order. A `serde_json` map would order its keys by
/// whether another crate in the build enabled `serde_json/preserve_order`,
/// and the token's bytes would move with it. The order is the sorted one,
/// which is what a map gives without that feature.
#[derive(serde::Serialize)]
struct Claims<'a> {
    aud: String,
    exp: u64,
    iat: u64,
    iss: &'a str,
    nbf: u64,
    scope: DelegationScope,
    sub: &'a str,
}

fn header(algorithm: DelegationAlgorithm) -> &'static [u8] {
    match algorithm {
        DelegationAlgorithm::Es256k => br#"{"alg":"ES256K","typ":"vera-delegation-v1+jwt"}"#,
        DelegationAlgorithm::EdDsa => br#"{"alg":"EdDSA","typ":"vera-delegation-v1+jwt"}"#,
        DelegationAlgorithm::Es256 => br#"{"alg":"ES256","typ":"vera-delegation-v1+jwt"}"#,
    }
}

/// The `did:key` of `key` from its compressed or its uncompressed encoding.
pub(super) fn did_from_key(key: &VerifyingKey, compressed: bool) -> String {
    let encoded_key = key.to_sec1_point(compressed);
    // multicodec prefix for secp256k1-pub: 0xe7 0x01
    let mut multicodec = vec![0xe7, 0x01];
    multicodec.extend_from_slice(encoded_key.as_bytes());
    let encoded = bs58::encode(&multicodec).into_string();
    format!("did:key:z{}", encoded)
}

#[cfg(test)]
pub(super) fn did_from_signing_key(
    key: &vera_client::Secp256k1SigningKey,
    compressed: bool,
) -> String {
    did_from_key(key.verifying_key(), compressed)
}

fn base64url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BearerError {
    #[error("issuer DID does not match the signing key")]
    Issuer,

    #[error("system time error: {0}")]
    Time(String),

    #[error("delegation claims could not be encoded: {0}")]
    Claims(String),

    #[error(transparent)]
    Signer(#[from] SignerError),
}

#[cfg(test)]
mod key_types;
#[cfg(test)]
mod pinned;

#[cfg(test)]
mod tests {
    use base64::Engine;
    use k256::ecdsa::signature::hazmat::PrehashVerifier;
    use k256::ecdsa::{Signature, VerifyingKey};
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use vera_client::Secp256k1SigningKey as SigningKey;

    use super::*;
    use crate::vera_rs::signer::Secp256k1Signer;

    #[nuclei::test]
    async fn bearer_token_uses_vera_rs_es256k_shape() {
        let signing_key = SigningKey::from_slice(&[7u8; 32]).expect("valid signing key");
        let signer = Secp256k1Signer::from_slice(&[7u8; 32]).expect("valid signing key");
        let subject = "did:key:zSubject\",\"scope\":\"other";
        let issuer = did_from_signing_key(&signing_key, false);
        let token = create_bearer_token(&signer, &issuer, subject, 9001, 300)
            .await
            .expect("bearer token");
        let parts: Vec<_> = token.split('.').collect();
        assert_eq!(parts.len(), 3);

        let decoder = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let header: Value = serde_json::from_slice(&decoder.decode(parts[0]).expect("header b64"))
            .expect("header json");
        assert_eq!(header["alg"], "ES256K");
        assert_eq!(header["typ"], "vera-delegation-v1+jwt");

        let payload: Value =
            serde_json::from_slice(&decoder.decode(parts[1]).expect("payload b64"))
                .expect("payload json");
        assert_eq!(payload["sub"], subject);
        assert_eq!(payload["iss"], issuer);
        assert_eq!(payload["aud"], "vera:9001");
        assert_eq!(payload["scope"], "acp:policy");
        assert_eq!(
            payload["exp"].as_u64().unwrap() - payload["iat"].as_u64().unwrap(),
            300
        );

        let issuer = payload["iss"].as_str().expect("issuer");
        let did_bytes = bs58::decode(issuer.strip_prefix("did:key:z").expect("did:key"))
            .into_vec()
            .expect("did base58");
        let issuer_pubkey = signing_key.verifying_key().to_sec1_point(false);
        assert_eq!(&did_bytes[..2], &[0xe7, 0x01]);
        assert_eq!(&did_bytes[2..], issuer_pubkey.as_bytes());

        let sig_bytes = decoder.decode(parts[2]).expect("signature b64");
        let signature = Signature::from_slice(&sig_bytes).expect("signature bytes");
        let digest = Sha256::digest(format!("{}.{}", parts[0], parts[1]).as_bytes());
        VerifyingKey::from_sec1_bytes(issuer_pubkey.as_bytes())
            .expect("issuer public key")
            .verify_prehash(digest.as_ref(), &signature)
            .expect("signature verifies");
    }

    #[nuclei::test]
    async fn a_scoped_token_keeps_the_issuer_spelling_it_was_given() {
        let key = SigningKey::from_slice(&[7u8; 32]).expect("valid signing key");
        let signer = Secp256k1Signer::from_slice(&[7u8; 32]).expect("valid signing key");
        let decoder = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        for compressed in [false, true] {
            let issuer = did_from_signing_key(&key, compressed);
            let token = create_scoped_bearer_token(
                &signer,
                &issuer,
                "did:key:zSubject",
                9001,
                300,
                vera_client::DelegationScope::CreatePolicy,
            )
            .await
            .expect("scoped token");
            let payload: Value = serde_json::from_slice(
                &decoder
                    .decode(token.split('.').nth(1).expect("payload"))
                    .expect("payload b64"),
            )
            .expect("payload json");
            assert_eq!(payload["iss"], issuer);
            assert_eq!(payload["scope"], "acp:policy:create");
        }
    }

    #[nuclei::test]
    async fn invalid_lifetime_is_rejected() {
        let key = SigningKey::from_slice(&[7u8; 32]).unwrap();
        let signer = Secp256k1Signer::from_slice(&[7u8; 32]).unwrap();
        let issuer = did_from_signing_key(&key, true);
        for lifetime in [0, u64::MAX] {
            assert!(
                create_bearer_token(&signer, &issuer, "did:key:zSubject", 9001, lifetime)
                    .await
                    .is_err()
            );
        }
        assert!(
            create_bearer_token(&signer, &issuer, "did:key:zSubject", 9001, 300)
                .await
                .is_ok()
        );
        let other = SigningKey::from_slice(&[8u8; 32]).unwrap();
        let other_issuer = did_from_signing_key(&other, false);
        assert!(matches!(
            create_bearer_token(&signer, &other_issuer, "did:key:zSubject", 9001, 300).await,
            Err(BearerError::Issuer)
        ));
    }
}

/// The delegation a caller presents to a node so that the node's vera.rs ACP
/// acts for it: `signer` lets `audience.worker` submit its policy commands on
/// `audience.deployment` for `lifetime`.
///
/// A caller reads the audience from the node's light client status.
pub async fn caller_delegation(
    signer: &dyn DelegationSigner,
    audience: &acp::DelegationAudience,
    lifetime: std::time::Duration,
) -> Result<String, BearerError> {
    create_bearer_token(
        signer,
        &super::did_spelling::wire(signer.did()),
        &audience.worker,
        audience.deployment,
        lifetime.as_secs(),
    )
    .await
}

/// The token type vera.rs gives a delegation.
const DELEGATION_TYPE: &str = "vera-delegation-v1+jwt";

/// Whether `token` declares itself a vera.rs delegation. Nothing is verified.
pub(super) fn is_delegation(token: &str) -> bool {
    use base64::Engine;

    #[derive(serde::Deserialize)]
    struct Header {
        typ: String,
    }
    token
        .split('.')
        .next()
        .and_then(|header| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(header)
                .ok()
        })
        .and_then(|header| serde_json::from_slice::<Header>(&header).ok())
        .is_some_and(|header| header.typ == DELEGATION_TYPE)
}

/// The DID that signed the delegation `token`, in DefraDB's spelling, when
/// vera.rs would act under it now for `worker` on `deployment_id`: a current
/// delegation for policy commands and nothing narrower or wider.
pub(super) fn caller_of_delegation(
    token: &str,
    worker: &str,
    deployment_id: u64,
) -> Result<String, CallerDelegationError> {
    let claims = vera_crypto::jwt::verify_bearer_token(token)?;
    if claims.relay.is_some()
        || claims.request.is_some()
        || claims.scope != DelegationScope::PolicyCommands
    {
        return Err(CallerDelegationError::Kind);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CallerDelegationError::Clock)?
        .as_secs();
    claims.authorize(worker, deployment_id, now)?;
    Ok(super::did_spelling::defra(&claims.iss).into_owned())
}

/// Why a caller's delegation was refused.
#[derive(Debug, thiserror::Error)]
pub(super) enum CallerDelegationError {
    #[error(transparent)]
    Token(#[from] vera_crypto::jwt::JwtError),
    #[error("the delegation is not a plain delegation for policy commands")]
    Kind,
    #[error("the system clock is before the Unix epoch")]
    Clock,
}

#[cfg(test)]
mod caller_tests {
    use super::*;
    use crate::vera_rs::signer::Ed25519Signer;

    const WORKER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

    fn audience(deployment: u64) -> acp::DelegationAudience {
        acp::DelegationAudience {
            worker: WORKER.into(),
            deployment,
        }
    }

    #[nuclei::test]
    async fn a_delegation_names_its_signer_only_to_the_worker_and_deployment_it_was_made_for() {
        {
            let signer = Ed25519Signer::from_slice(&[7; 32]).unwrap();
            let lifetime = std::time::Duration::from_secs(60);
            let token = caller_delegation(&signer, &audience(9), lifetime)
                .await
                .unwrap();
            assert!(is_delegation(&token));
            assert_eq!(
                caller_of_delegation(&token, WORKER, 9).unwrap(),
                signer.did()
            );
            assert!(caller_of_delegation(&token, WORKER, 10).is_err());
            assert!(caller_of_delegation(&token, signer.did(), 9).is_err());

            let mut forged = token.clone();
            forged.pop();
            forged.push(if token.ends_with('A') { 'B' } else { 'A' });
            assert!(is_delegation(&forged));
            assert!(caller_of_delegation(&forged, WORKER, 9).is_err());

            let narrower = create_scoped_bearer_token(
                &signer,
                signer.did(),
                WORKER,
                9,
                60,
                DelegationScope::CreatePolicy,
            )
            .await
            .unwrap();
            assert!(matches!(
                caller_of_delegation(&narrower, WORKER, 9),
                Err(CallerDelegationError::Kind)
            ));
        }
    }

    #[test]
    fn an_identity_token_is_not_a_delegation() {
        let identity =
            identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap()).unwrap();
        let token = identity::new_token(
            &identity,
            std::time::Duration::from_secs(60),
            Some("127.0.0.1:9181".into()),
            None,
        )
        .unwrap();
        assert!(!is_delegation(std::str::from_utf8(&token).unwrap()));
        assert!(!is_delegation("not a token"));
    }
}
