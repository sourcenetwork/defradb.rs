//! One spelling per secp256k1 actor on this side of the vera.rs boundary.
//!
//! A secp256k1 `did:key` can be written from the compressed or the uncompressed
//! public key. Both name the same key. vera.rs treats every spelling a request
//! uses as one actor and records and returns that actor under the compressed
//! one. DefraDB derives the uncompressed one, as Go does, and compares actors
//! as strings.
//!
//! Every actor DID this provider sends or returns is therefore DefraDB's
//! spelling, including its own, and a DID read from vera.rs is compared with
//! [`same_actor`]. Other key types have a single spelling and pass through
//! unchanged.

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
            "did:key:zDnaerx9CtbPJ1q36T5Ln5wYt3MQYeGRG5ehnPAmxcf5mDZpv",
            "did:key:zNotAKey",
            "*",
            "",
        ] {
            assert_eq!(defra(did), did);
        }
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
