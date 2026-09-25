use std::time::{SystemTime, UNIX_EPOCH};

use k256::ecdsa::SigningKey;
use sha2::{Digest, Sha256};

/// Create an ES256K JWT bearer token for hub.rs ACP operations.
///
/// The token uses the `did:key:z...` format for the issuer.
pub fn create_bearer_token(
    signing_key: &SigningKey,
    issuer: &str,
    subject: &str,
    deployment_id: u64,
    expiry_secs: u64,
) -> Result<String, BearerError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| BearerError::Time(e.to_string()))?
        .as_secs();

    if issuer != did_from_signing_key(signing_key, true)
        && issuer != did_from_signing_key(signing_key, false)
    {
        return Err(BearerError::Issuer);
    }

    let expiry = now
        .checked_add(expiry_secs)
        .filter(|expiry| *expiry > now)
        .ok_or_else(|| BearerError::Time("invalid delegation lifetime".into()))?;
    let header = base64url_encode(br#"{"alg":"ES256K","typ":"vera-delegation-v1+jwt"}"#);
    let payload_json = serde_json::json!({
        "iss": issuer,
        "sub": subject,
        "aud": format!("vera:{deployment_id}"),
        "scope": "acp:policy",
        "iat": now,
        "nbf": now.saturating_sub(30),
        "exp": expiry,
    })
    .to_string();
    let payload = base64url_encode(payload_json.as_bytes());

    let message = format!("{}.{}", header, payload);
    let digest = Sha256::digest(message.as_bytes());
    let (signature, _) = signing_key
        .sign_prehash_recoverable(digest.as_ref())
        .map_err(|e| BearerError::Sign(e.to_string()))?;
    let sig_b64 = base64url_encode(&signature.to_bytes());

    Ok(format!("{}.{}", message, sig_b64))
}

pub(super) fn did_from_signing_key(key: &SigningKey, compressed: bool) -> String {
    let verifying_key = key.verifying_key();
    let encoded_key = verifying_key.to_encoded_point(compressed);
    // multicodec prefix for secp256k1-pub: 0xe7 0x01
    let mut multicodec = vec![0xe7, 0x01];
    multicodec.extend_from_slice(encoded_key.as_bytes());
    let encoded = bs58::encode(&multicodec).into_string();
    format!("did:key:z{}", encoded)
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

    #[error("signing error: {0}")]
    Sign(String),
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use k256::ecdsa::signature::hazmat::PrehashVerifier;
    use k256::ecdsa::{Signature, SigningKey};
    use serde_json::Value;

    use super::*;

    #[test]
    fn bearer_token_uses_hub_rs_es256k_shape() {
        let signing_key = SigningKey::from_slice(&[7u8; 32]).expect("valid signing key");
        let subject = "did:key:zSubject\",\"scope\":\"other";
        let issuer = did_from_signing_key(&signing_key, false);
        let token =
            create_bearer_token(&signing_key, &issuer, subject, 9001, 300).expect("bearer token");
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
        let issuer_pubkey = signing_key.verifying_key().to_encoded_point(false);
        assert_eq!(&did_bytes[..2], &[0xe7, 0x01]);
        assert_eq!(&did_bytes[2..], issuer_pubkey.as_bytes());

        let sig_bytes = decoder.decode(parts[2]).expect("signature b64");
        let signature = Signature::from_bytes((&sig_bytes[..]).into()).expect("signature bytes");
        let digest = Sha256::digest(format!("{}.{}", parts[0], parts[1]).as_bytes());
        signing_key
            .verifying_key()
            .verify_prehash(digest.as_ref(), &signature)
            .expect("signature verifies");
    }

    #[test]
    fn invalid_lifetime_is_rejected() {
        let key = SigningKey::from_slice(&[7u8; 32]).unwrap();
        let issuer = did_from_signing_key(&key, true);
        for lifetime in [0, u64::MAX] {
            assert!(
                create_bearer_token(&key, &issuer, "did:key:zSubject", 9001, lifetime).is_err()
            );
        }
        assert!(create_bearer_token(&key, &issuer, "did:key:zSubject", 9001, 300).is_ok());
        let other = SigningKey::from_slice(&[8u8; 32]).unwrap();
        let other_issuer = did_from_signing_key(&other, false);
        assert!(create_bearer_token(&key, &other_issuer, "did:key:zSubject", 9001, 300).is_err());
    }
}
