//! The client identities a test acts as, one per seed: the same seed is the
//! same identity in every test and through every client.

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
