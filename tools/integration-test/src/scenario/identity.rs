//! The client identities a test acts as, one per seed: the same seed is the
//! same identity in every test and through every client.
//!
//! Go's harness (`tests/state/identity.go`, `GetIdentityHolder`) gives each
//! `ClientIdentity(n)` one secp256k1 identity per test, made the first time
//! it is used, and sends it as a bearer token to an HTTP node. This does the
//! same per seed. Go draws the key from a counter of identities made so far,
//! so its DIDs depend on the order a test first uses them; the scenarios
//! match DIDs by shape, never by value, so only one key per seed matters.

use ::identity::{Identity as _, KeyType, RawIdentity};

/// Seeds from here on name node identities: `NODE_IDENTITY_BASE + n` acts as
/// Go's `NodeIdentity(n)` through every client, as an `identityNode`
/// argument asks.
pub const NODE_IDENTITY_BASE: u64 = 1 << 48;

/// Seeds from here to `NODE_IDENTITY_BASE` name Ed25519 client identities.
pub const ED25519_IDENTITY_BASE: u64 = 1 << 47;

/// The secp256k1 private key of client identity `seed`. Any 32 bytes below
/// the curve order are a key, and these are.
pub(crate) fn private_key(seed: u64) -> [u8; 32] {
    assert!(
        ed25519_private_key(seed).is_none(),
        "an Ed25519 client identity has 64 bytes"
    );
    if let Some(node) = seed.checked_sub(NODE_IDENTITY_BASE) {
        return node_private_key(node);
    }
    let mut key = [0x11; 32];
    key[24..].copy_from_slice(&seed.to_be_bytes());
    key
}

/// Client identity `seed`.
pub fn identity(seed: u64) -> RawIdentity {
    match ed25519_private_key(seed) {
        Some(key) => RawIdentity::from_bytes(KeyType::Ed25519, &key).expect("a valid Ed25519 key"),
        None => RawIdentity::from_bytes(KeyType::Secp256k1, &private_key(seed))
            .expect("a valid secp256k1 key"),
    }
}

/// The DID of client identity `seed`.
pub fn did(seed: u64) -> String {
    identity(seed).did().expect("a key has a DID").to_string()
}

/// The private key of client identity `seed` as `defra client --identity`
/// reads it.
pub fn private_key_hex(seed: u64) -> String {
    match ed25519_private_key(seed) {
        Some(key) => hex::encode(key),
        None => hex::encode(private_key(seed)),
    }
}

/// The local signing key of client identity `seed`, as an in-process caller
/// attaches it to a write: Go's `FullIdentity`.
pub fn signing_config(seed: u64) -> defra_core::signing::SigningConfig {
    if let Some(private_key) = ed25519_private_key(seed) {
        let public = identity(seed).public_key_bytes();
        return defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: private_key,
            public_key_hex: hex::encode(&public),
            public_key_bytes: public,
            remote_signer: None,
            signing_authorization: None,
        };
    }
    let public = identity(seed).public_key_bytes();
    defra_core::signing::SigningConfig {
        key_type: defra_core::signing::SigningKeyType::Secp256k1,
        private_key_bytes: private_key(seed).to_vec(),
        public_key_hex: hex::encode(&public),
        public_key_bytes: public,
        remote_signer: None,
        signing_authorization: None,
    }
}

pub(crate) fn ed25519_private_key(seed: u64) -> Option<Vec<u8>> {
    if seed >= NODE_IDENTITY_BASE {
        return None;
    }
    let local = seed.checked_sub(ED25519_IDENTITY_BASE)?;
    let mut bytes = [0x33; 32];
    bytes[24..].copy_from_slice(&local.to_be_bytes());
    Some(crypto::ed25519_key_from_seed(&bytes).expect("any 32 bytes are an Ed25519 seed"))
}

/// The keys Go's harness makes for its first identities (`generateIdentity`,
/// `math/rand` seeded with how many it made before): the same 32 bytes are
/// the secp256k1 key and the Ed25519 seed. Printed by Go at the parity pin.
const GO_HARNESS_KEYS: [&str; 8] = [
    "0194fdc2fa2ffcc041d3ff12045b73c86e4ff95ff662a5eee82abdf44a2d0b75",
    "52fdfc072182654f163f5f0f9a621d729566c74d10037c4d7bbb0407d1e2c649",
    "2f8282cbe2f9696f3144c0aa4ced56dbd967dc2897806af3bed8a63aca16e18b",
    "85fbe72b6064289004a531f967898df5319ee02992fdd84021fa5052434bf6ee",
    "e2807d9c1dce26af00ca81d4fe11c23e8eb6752e1f9ad716c61fc24f2d80c041",
    "c00913e02a63e4cf532d9b2ce282fad85af699815c18c595ea804462a794f751",
    "158fe87f5cf3dff06e50dbd3e81b2242314ba86c2b962f6c0b9e0e91a04496c0",
    "f3ff4d451e429e182215aaee06a2d64b6d1aadc9e5031e4b99bf11ae0a796ebc",
];

/// The key of Go's `NodeIdentity(seed)`, made as node `seed` starts, so the
/// identity numbered `seed` when every node starts before any client
/// identity is used.
// ponytail: assumes nodes start before client identities are first used; a
// test that interleaves them needs Go's full identity counter.
fn go_node_key(seed: u64) -> Option<[u8; 32]> {
    let key = GO_HARNESS_KEYS.get(usize::try_from(seed).ok()?)?;
    let mut bytes = [0u8; 32];
    hex::decode_to_slice(key, &mut bytes).ok()?;
    Some(bytes)
}

/// The private key of node identity `seed`, Go's `NodeIdentity(seed)`: an
/// identity of its own, never one of the client identities.
pub fn node_private_key(seed: u64) -> [u8; 32] {
    if let Some(key) = go_node_key(seed) {
        return key;
    }
    let mut key = [0x22; 32];
    key[24..].copy_from_slice(&seed.to_be_bytes());
    key
}

/// The DID of node identity `seed`.
pub fn node_did(seed: u64) -> String {
    RawIdentity::from_bytes(KeyType::Secp256k1, &node_private_key(seed))
        .and_then(|identity| identity.did())
        .expect("a valid secp256k1 key has a DID")
        .to_string()
}
