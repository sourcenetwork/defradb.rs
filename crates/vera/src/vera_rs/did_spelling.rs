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
//! Every actor DID this provider keeps or returns is therefore DefraDB's
//! spelling ([`defra`]), including its own, and a DID read from vera.rs is
//! compared with [`same_actor`]. A DID sent to vera.rs is written as vera.rs
//! takes it ([`wire`]). Ed25519 keys have a single spelling and pass through
//! unchanged.

use std::borrow::Cow;

use crypto::{create_did_key, parse_did_key, KeyType};

/// The spelling DefraDB derives for the key `did` names.
pub(crate) fn defra(did: &str) -> Cow<'_, str> {
    respelled(did, false, false)
}

/// The spelling vera.rs is sent for the key `did` names: DefraDB's for a
/// secp256k1 key, which vera.rs takes in any spelling, and the compressed one
/// for a P-256 key, which it takes in no other.
pub(crate) fn wire(did: &str) -> Cow<'_, str> {
    respelled(did, false, true)
}

/// `did` with its secp256k1 or P-256 point written compressed or not.
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
            _ => None,
        };
    match point.map(|(kind, bytes)| create_did_key(kind, &bytes)) {
        Some(Ok(spelling)) if spelling != did => Cow::Owned(spelling),
        _ => Cow::Borrowed(did),
    }
}

/// Whether `a` and `b` name one actor, whichever spelling each uses.
pub(crate) fn same_actor(a: &str, b: &str) -> bool {
    defra(a) == defra(b)
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

        let other = vera_client::Secp256k1SigningKey::from_slice(&[43; 32]).expect("valid key");
        for compressed_other in [true, false] {
            let other = crate::vera_rs::bearer::did_from_signing_key(&other, compressed_other);
            assert!(!same_actor(&compressed, &other));
            assert!(!same_actor(&uncompressed, &other));
        }

        let ed25519 = "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp";
        assert!(same_actor(ed25519, ed25519));
        assert!(!same_actor(ed25519, &compressed));
        assert!(!same_actor("did:key:zNotAKey", "did:key:zAnother"));
    }
}
