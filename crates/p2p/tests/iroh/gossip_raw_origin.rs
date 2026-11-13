//! A sealed raw payload opens back to its origin and payload, a forged or
//! retargeted one is refused, and an unsealed one passes through untouched,
//! as `proofs/lean/GossipFrame/RawOrigin.lean` proves for the layout.

use super::*;

const TOPIC: &str = "encryption";

#[test]
fn sealed_payload_opens_to_its_origin() {
    let key = SecretKey::generate();
    let sealed = seal(&key, TOPIC, b"request").unwrap();
    assert_eq!(
        open(TOPIC, &sealed).unwrap(),
        Opened::Signed {
            origin: key.public(),
            payload: b"request",
        }
    );
}

#[test]
fn empty_payload_round_trips() {
    let key = SecretKey::generate();
    let sealed = seal(&key, TOPIC, b"").unwrap();
    assert_eq!(
        open(TOPIC, &sealed).unwrap(),
        Opened::Signed {
            origin: key.public(),
            payload: b"",
        }
    );
}

#[test]
fn a_sealed_payload_is_bound_to_its_topic() {
    let sealed = seal(&SecretKey::generate(), TOPIC, b"request").unwrap();
    assert!(matches!(
        open("sync-branchable", &sealed),
        Err(Error::InvalidSignature)
    ));
}

#[test]
fn a_claimed_origin_without_its_key_is_refused() {
    let claimed = SecretKey::generate();
    let forger = SecretKey::generate();
    let mut sealed = seal(&forger, TOPIC, b"request").unwrap();
    sealed[MAGIC.len()..MAGIC.len() + KEY_LEN].copy_from_slice(claimed.public().as_bytes());
    assert!(open(TOPIC, &sealed).is_err());
}

#[test]
fn a_tampered_payload_is_refused() {
    let mut sealed = seal(&SecretKey::generate(), TOPIC, b"request").unwrap();
    let last = sealed.len() - 1;
    sealed[last] ^= 1;
    assert!(matches!(open(TOPIC, &sealed), Err(Error::InvalidSignature)));
}

#[test]
fn a_truncated_seal_is_refused_not_passed_through() {
    let sealed = seal(&SecretKey::generate(), TOPIC, b"request").unwrap();
    assert!(matches!(
        open(TOPIC, &sealed[..HEADER_LEN - 1]),
        Err(Error::InvalidSignature)
    ));
}

#[test]
fn an_unsealed_cbor_payload_passes_through() {
    // A bare CBOR map, `{"a": 1}`, the shape every raw request starts with.
    let request = [0xA1, 0x61, b'a', 0x01];
    assert_eq!(open(TOPIC, &request).unwrap(), Opened::Unsigned(&request));
    assert_eq!(open(TOPIC, b"").unwrap(), Opened::Unsigned(b""));
}
