//! The node's at-rest keys, handed to the engine from memory.
//!
//! A sealed file records the id of the key that sealed it, so an id must name
//! the same bytes for as long as any file names it. Each key's id is derived
//! from its bytes, which keeps that true without storing anything: rotating to
//! a new key gives it a new id, and the old key, kept in the older set, still
//! answers for the files it sealed until a compaction re-seals them.

use regolith::{KeyId, KeyMaterial, KeyProvider};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::corekv::{Error, Result};

/// The current key, which seals every new file, and the older keys files may
/// still name. The current key is first.
pub(crate) struct AtRestKeys {
    current: (KeyId, Zeroizing<[u8; 32]>),
    older: Vec<(KeyId, Zeroizing<[u8; 32]>)>,
}

impl AtRestKeys {
    /// One key, sealing and opening everything.
    pub(crate) fn single(key: [u8; 32]) -> Self {
        Self {
            current: (key_id(&key), Zeroizing::new(key)),
            older: Vec::new(),
        }
    }

    /// `current` seals new files; `older` stay readable. A repeated key is
    /// kept once. Two different keys whose ids collide are refused, because
    /// the engine could not tell their files apart.
    pub(crate) fn rotated<I>(current: [u8; 32], older: I) -> Result<Self>
    where
        I: IntoIterator<Item = [u8; 32]>,
    {
        let mut keys = Self::single(current);
        for key in older {
            let id = key_id(&key);
            let same_key = keys
                .entries()
                .find(|(held, _)| *held == id)
                .map(|(_, held)| **held == key);
            match same_key {
                Some(true) => {}
                Some(false) => {
                    return Err(Error::Other(
                        "two at-rest keys share a key id; replace one of them".into(),
                    ))
                }
                None => keys.older.push((id, Zeroizing::new(key))),
            }
        }
        Ok(keys)
    }

    /// Every key, the current one first.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn all(&self) -> impl Iterator<Item = &[u8; 32]> {
        self.entries().map(|(_, key)| &**key)
    }

    fn entries(&self) -> impl Iterator<Item = &(KeyId, Zeroizing<[u8; 32]>)> {
        std::iter::once(&self.current).chain(&self.older)
    }
}

/// The first four bytes of a domain-separated SHA-256 of the key: a key check
/// value, stable for the key's lifetime and never the key itself.
fn key_id(key: &[u8; 32]) -> KeyId {
    let digest = Sha256::new()
        .chain_update(b"defradb at-rest key id\0")
        .chain_update(key)
        .finalize();
    KeyId(u32::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3],
    ]))
}

impl KeyProvider for AtRestKeys {
    fn current(&self) -> KeyId {
        self.current.0
    }

    fn key(&self, id: KeyId) -> Option<KeyMaterial> {
        self.entries()
            .find(|(held, _)| *held == id)
            .map(|(_, key)| KeyMaterial::new(**key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_answers_for_its_own_id_and_the_first_is_current() {
        let keys = AtRestKeys::rotated([1; 32], [[2; 32], [3; 32], [2; 32]]).unwrap();
        assert_eq!(keys.all().count(), 3);
        assert_eq!(keys.current(), key_id(&[1; 32]));
        for key in [[1u8; 32], [2; 32], [3; 32]] {
            assert!(keys.key(key_id(&key)).is_some());
        }
        assert!(keys.key(key_id(&[4; 32])).is_none());
    }

    #[test]
    fn two_different_keys_with_one_id_are_refused() {
        use rapidhash::fast::RapidHashMap;
        use rapidhash::HashMapExt;

        // A birthday search over 32-bit ids finds a pair in about 2^16 tries.
        let mut seen = RapidHashMap::with_capacity(1 << 17);
        let (first, second) = (0u64..)
            .find_map(|i| {
                let mut key = [0u8; 32];
                key[..8].copy_from_slice(&i.to_be_bytes());
                seen.insert(key_id(&key), key).map(|earlier| (earlier, key))
            })
            .unwrap();
        assert!(AtRestKeys::rotated(first, [second]).is_err());
        assert!(AtRestKeys::rotated(first, [first]).is_ok());
    }

    #[test]
    fn a_key_keeps_its_id_across_providers() {
        let first = AtRestKeys::single([9; 32]);
        let rotated = AtRestKeys::rotated([8; 32], [[9; 32]]).unwrap();
        assert_ne!(rotated.current(), first.current());
        assert!(rotated.key(first.current()).is_some());
    }
}
