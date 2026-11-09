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

/// The secp256k1 private key of client identity `seed`. Any 32 bytes below
/// the curve order are a key, and these are.
fn private_key(seed: u64) -> [u8; 32] {
    let mut key = [0x11; 32];
    key[24..].copy_from_slice(&seed.to_be_bytes());
    key
}

/// Client identity `seed`.
pub fn identity(seed: u64) -> RawIdentity {
    RawIdentity::from_bytes(KeyType::Secp256k1, &private_key(seed)).expect("a valid secp256k1 key")
}

/// The DID of client identity `seed`.
pub fn did(seed: u64) -> String {
    identity(seed).did().expect("a key has a DID").to_string()
}

/// The private key of client identity `seed` as `defra client --identity`
/// reads it.
pub fn private_key_hex(seed: u64) -> String {
    hex::encode(private_key(seed))
}

/// The private key of node identity `seed`, Go's `NodeIdentity(seed)`: an
/// identity of its own, never one of the client identities.
pub fn node_private_key(seed: u64) -> [u8; 32] {
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
