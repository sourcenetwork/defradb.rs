//! Which block signature types a Go peer can verify.
//!
//! Go's `getPublicKeyFromSignature` (`internal/core/block/signature.go:186`)
//! maps only `EdDSA` and `ES256K` to a key type and returns
//! `ErrUnsupportedPrivKeyType` for anything else. A block signed with any other
//! type is refused during replication, so which types are Rust-only is part of
//! the wire contract and belongs in a test rather than a comment: the comment
//! that recorded it was deleted once already.

use defra_core::block::SignatureType;

/// The two types Go's verifier accepts.
#[test]
fn go_verifiable_types_match_gos_verifier() {
    assert!(SignatureType::ES256K.is_go_verifiable());
    assert!(SignatureType::EdDSA.is_go_verifiable());
}

/// Rust-only types. `BLSAugV1` is the Orbis ring extension;
/// `ES256` covers secp256r1, including Secure Enclave keys.
#[test]
fn rust_only_types_are_not_go_verifiable() {
    assert!(!SignatureType::BLSAugV1.is_go_verifiable());
    assert!(!SignatureType::ES256.is_go_verifiable());
}

/// Every variant is classified. A new signature type cannot be added without
/// deciding whether Go peers can consume it, because `is_go_verifiable` matches
/// exhaustively and this walks the same set.
#[test]
fn every_signature_type_is_classified() {
    let all = [
        SignatureType::ES256K,
        SignatureType::EdDSA,
        SignatureType::ES256,
        SignatureType::BLSAugV1,
    ];
    let go_verifiable = all.iter().filter(|kind| kind.is_go_verifiable()).count();

    assert_eq!(
        go_verifiable, 2,
        "exactly the two types Go maps to a key type are wire compatible"
    );
    assert_eq!(all.len(), 4, "a new variant needs a decision in this test");
}

#[test]
fn augmented_bls_has_a_distinct_serialized_signature_tag() {
    use defra_core::block::SignatureHeader;
    use defra_core::signing::SigningKeyType;
    use ipld_core::ipld::Ipld;
    let kind: SigningKeyType = "bls_aug_v1".parse().unwrap();
    assert_eq!(kind.as_str(), "bls_aug_v1");
    assert_eq!(kind.to_signature_type(), SignatureType::BLSAugV1);
    assert_eq!(serde_json::to_string(&kind).unwrap(), "\"bls_aug_v1\"");
    let header = SignatureHeader::new(SignatureType::BLSAugV1, b"key".to_vec());
    let encoded = serde_json::to_value(&header).unwrap();
    assert_eq!(encoded["type"], "BLS_AUG_V1");
    assert_eq!(
        SignatureHeader::try_from(&Ipld::from(&header)).unwrap(),
        header
    );
    assert!(serde_json::from_value::<SignatureType>(serde_json::json!("BLS")).is_err());
    assert!("bls".parse::<SigningKeyType>().is_err());
    assert!(serde_json::from_value::<SigningKeyType>(serde_json::json!("bls")).is_err());
    let mut obsolete = Ipld::from(&header);
    let Ipld::Map(fields) = &mut obsolete else {
        panic!("signature header must be a map")
    };
    fields.insert("type".into(), Ipld::String("BLS".into()));
    assert!(SignatureHeader::try_from(&obsolete).is_err());
}
