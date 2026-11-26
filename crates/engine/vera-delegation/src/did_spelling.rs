//! One spelling per actor on each side of the vera.rs boundary.
//!
//! A secp256k1 or P-256 `did:key` can be written from the compressed or the
//! uncompressed public key. Both name the same key. DefraDB derives the
//! uncompressed one, as Go does, and compares actors as strings.
//!
//! vera.rs treats every spelling of a secp256k1 key as one actor and records
//! and returns it under the compressed one. It accepts a P-256 key under the
//! compressed spelling only, and compares it as written.
//!
//! Every actor DID the vera.rs provider keeps or returns is therefore DefraDB's
//! spelling ([`defra`]), including its own, and a DID read from vera.rs is
//! compared with [`same_actor`]. A DID sent to vera.rs is written as vera.rs
//! takes it ([`wire`]).
//!
//! Any of these keys may also be written in base64url (`did:key:u…`) rather
//! than base58btc (`did:key:z…`). vera.rs takes a secp256k1 key in either and
//! the other types in base58btc only, and DefraDB derives base58btc, so both
//! [`defra`] and [`wire`] write base58btc, an Ed25519 key's one encoding
//! included.

use std::borrow::Cow;

use crypto::{create_did_key, parse_did_key, KeyType};

/// The spelling DefraDB derives for the key `did` names.
pub fn defra(did: &str) -> Cow<'_, str> {
    respelled(did, false, false)
}

/// The spelling vera.rs is sent for the key `did` names: DefraDB's for a
/// secp256k1 key, which vera.rs takes in any spelling, and the compressed one
/// for a P-256 key, which it takes in no other.
pub fn wire(did: &str) -> Cow<'_, str> {
    respelled(did, false, true)
}

/// `did` in base58btc, with its secp256k1 or P-256 point written compressed
/// or not.
fn respelled(did: &str, compress_secp256k1: bool, compress_p256: bool) -> Cow<'_, str> {
    let point =
        match parse_did_key(did) {
            Ok((KeyType::Secp256k1, key)) => k256::ecdsa::VerifyingKey::from_sec1_bytes(&key)
                .ok()
                .map(|key| {
                    let point = key.to_sec1_point(compress_secp256k1);
                    (KeyType::Secp256k1, point.as_bytes().to_vec())
                }),
            Ok((KeyType::Secp256r1, key)) => p256::ecdsa::VerifyingKey::from_sec1_bytes(&key)
                .ok()
                .map(|key| {
                    let point = key.to_sec1_point(compress_p256);
                    (KeyType::Secp256r1, point.as_bytes().to_vec())
                }),
            Ok((KeyType::Ed25519, key)) if key.len() == 32 => Some((KeyType::Ed25519, key)),
            _ => None,
        };
    match point.map(|(kind, bytes)| create_did_key(kind, &bytes)) {
        Some(Ok(spelling)) if spelling != did => Cow::Owned(spelling),
        _ => Cow::Borrowed(did),
    }
}

/// Every spelling this module knows for the key `did` names, `did` itself
/// first: a key the node holds may be registered under any of them.
pub fn spellings(did: &str) -> Vec<Cow<'_, str>> {
    let mut all = vec![Cow::Borrowed(did)];
    for other in [respelled(did, false, false), respelled(did, true, true)] {
        if !all.contains(&other) {
            all.push(other);
        }
    }
    all
}

/// Whether `a` and `b` name one actor, whichever spelling each uses.
pub fn same_actor(a: &str, b: &str) -> bool {
    defra(a) == defra(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spellings() -> (String, String) {
        let key = k256::ecdsa::SigningKey::from_slice(&[42; 32]).expect("valid key");
        (
            crate::bearer::did_from_signing_key(&key, true),
            crate::bearer::did_from_signing_key(&key, false),
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
    fn a_base64url_spelling_meets_the_base58btc_ones() {
        use base64::Engine as _;

        let (compressed, uncompressed) = spellings();
        for did in [&compressed, &uncompressed] {
            let bytes = bs58::decode(&did["did:key:z".len()..])
                .into_vec()
                .expect("base58btc");
            let base64url = format!(
                "did:key:u{}",
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
            );
            assert_eq!(defra(&base64url), uncompressed);
            assert_eq!(wire(&base64url), uncompressed);
            assert!(same_actor(&base64url, &compressed));
        }

        let ed25519 = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";
        let bytes = bs58::decode(&ed25519["did:key:z".len()..])
            .into_vec()
            .expect("base58btc");
        let base64url = format!(
            "did:key:u{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        );
        assert_eq!(defra(&base64url), ed25519);
        assert_eq!(wire(&base64url), ed25519);
        assert!(same_actor(&base64url, ed25519));
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
            "did:key:zNotAKey",
            "*",
            "",
        ] {
            assert_eq!(defra(did), did);
            assert_eq!(wire(did), did);
        }
    }

    #[test]
    fn a_p256_actor_is_kept_uncompressed_and_sent_compressed() {
        // The W3C did:key vector for P-256, which is the compressed spelling.
        let compressed = "did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv";
        let uncompressed = defra(compressed).into_owned();
        assert_ne!(uncompressed, compressed);
        assert_eq!(parse_did_key(&uncompressed).expect("did:key").1.len(), 65);
        assert_eq!(defra(&uncompressed), uncompressed);
        assert_eq!(wire(&uncompressed), compressed);
        assert!(matches!(wire(compressed), Cow::Borrowed(_)));
        assert!(same_actor(compressed, &uncompressed));
    }

    #[test]
    fn every_spelling_of_a_key_is_listed_once_and_the_given_one_first() {
        let (compressed, uncompressed) = spellings();
        assert_eq!(
            super::spellings(&compressed),
            [compressed.as_str(), uncompressed.as_str()]
        );
        assert_eq!(
            super::spellings(&uncompressed),
            [uncompressed.as_str(), compressed.as_str()]
        );
        let ed25519 = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";
        assert_eq!(super::spellings(ed25519), [ed25519]);
    }

    #[test]
    fn a_secp256k1_actor_is_sent_as_it_is_kept() {
        let (compressed, uncompressed) = spellings();
        assert_eq!(wire(&compressed), uncompressed);
        assert!(matches!(wire(&uncompressed), Cow::Borrowed(_)));
    }

    #[test]
    fn an_actor_vera_rs_names_in_the_compressed_spelling_is_the_same_actor() {
        let (compressed, uncompressed) = spellings();
        assert!(same_actor(&compressed, &uncompressed));
        assert!(same_actor(&uncompressed, &compressed));
        assert!(same_actor(&uncompressed, &uncompressed));

        let other = k256::ecdsa::SigningKey::from_slice(&[43; 32]).expect("valid key");
        for compressed_other in [true, false] {
            let other = crate::bearer::did_from_signing_key(&other, compressed_other);
            assert!(!same_actor(&compressed, &other));
            assert!(!same_actor(&uncompressed, &other));
        }

        let ed25519 = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";
        assert!(same_actor(ed25519, ed25519));
        assert!(!same_actor(ed25519, &compressed));
        assert!(!same_actor("did:key:zNotAKey", "did:key:zAnother"));
    }
}
