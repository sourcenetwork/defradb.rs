//! One spelling per secp256k1 actor at the vera.rs boundary.
//!
//! A secp256k1 `did:key` can be written from the compressed or the uncompressed
//! public key. Both name the same key, but vera.rs records an actor under the
//! spelling it was given and compares policy owners and relationship subjects
//! as strings, so an actor that reached it under both would be two subjects.
//!
//! DefraDB derives the uncompressed spelling, as Go does, and documents are
//! owned on vera.rs under it. Every actor DID this provider sends or returns
//! is therefore that spelling, including its own. Other key types have a
//! single spelling and pass through unchanged.

use std::borrow::Cow;

use crypto::{create_did_key, parse_did_key, KeyType};

/// The spelling DefraDB derives for the key `did` names.
pub(crate) fn defra(did: &str) -> Cow<'_, str> {
    let Ok((KeyType::Secp256k1, key)) = parse_did_key(did) else {
        return Cow::Borrowed(did);
    };
    let Ok(key) = k256::ecdsa::VerifyingKey::from_sec1_bytes(&key) else {
        return Cow::Borrowed(did);
    };
    match create_did_key(KeyType::Secp256k1, key.to_sec1_point(false).as_bytes()) {
        Ok(spelling) if spelling != did => Cow::Owned(spelling),
        _ => Cow::Borrowed(did),
    }
}

/// Whether a delegation token names its issuer as this provider names actors.
///
/// A token signed elsewhere cannot be respelled. Under the compressed spelling
/// of a secp256k1 issuer, vera.rs would record a second actor that the grants
/// and checks sent through this provider never name. A token that does not
/// decode is left for vera.rs to reject.
pub(crate) fn issuer_is_canonical(token: &str) -> bool {
    use base64::Engine as _;

    #[derive(serde::Deserialize)]
    struct Claims {
        iss: String,
    }
    let claims = token.split('.').nth(1).and_then(|payload| {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .ok()?;
        serde_json::from_slice::<Claims>(&bytes).ok()
    });
    claims.is_none_or(|claims| matches!(defra(&claims.iss), Cow::Borrowed(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spellings() -> (String, String) {
        let key = vera_client::Secp256k1SigningKey::from_slice(&[42; 32]).expect("valid key");
        (
            crate::vera_rs::bearer::did_from_signing_key(&key, true),
            crate::vera_rs::bearer::did_from_signing_key(&key, false),
        )
    }

    #[test]
    fn both_spellings_of_a_secp256k1_key_meet_at_the_uncompressed_did() {
        let (compressed, uncompressed) = spellings();
        assert_ne!(compressed, uncompressed);
        assert_eq!(defra(&compressed), uncompressed);
        assert_eq!(defra(&uncompressed), uncompressed);
        assert!(matches!(defra(&uncompressed), Cow::Borrowed(_)));
    }

    #[test]
    fn the_uncompressed_spelling_is_the_one_defradb_derives() {
        use identity::Identity as _;

        let private = crypto::generate_secp256k1().expect("key");
        let identity = identity::RawIdentity::from_secp256k1(private).expect("identity");
        let did = identity.did().expect("did").to_string();
        assert!(matches!(defra(&did), Cow::Borrowed(_)));
    }

    #[test]
    fn other_identifiers_pass_through_unchanged() {
        for did in [
            "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp",
            "did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv",
            "did:key:zNotAKey",
            "*",
            "",
        ] {
            assert_eq!(defra(did), did);
        }
    }

    #[test]
    fn a_token_must_name_its_secp256k1_issuer_in_the_uncompressed_spelling() {
        use base64::Engine as _;
        let token = |issuer: &str| {
            let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::json!({ "iss": issuer }).to_string());
            format!("header.{payload}.signature")
        };
        let (compressed, uncompressed) = spellings();
        assert!(issuer_is_canonical(&token(&uncompressed)));
        assert!(!issuer_is_canonical(&token(&compressed)));
        assert!(issuer_is_canonical(&token(
            "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp"
        )));
        assert!(issuer_is_canonical("device.jwt.token"));
    }
}
