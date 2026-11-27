//! The read side of the previous release's per-value encryption wrapper
//! (`crates/engine/storage/src/encrypted_store.rs`, deleted by #1920).
//!
//! The wrapper stored each value as a 12-byte random nonce followed by its
//! AES-256-GCM ciphertext and 16-byte tag, with the value's full storage key
//! as associated data, so a value moved to another key fails to open. Only
//! decryption is kept, for the one-time rewrite.

/// Bytes a value grew by under the wrapper: the nonce and the tag.
pub(super) const OVERHEAD: usize = 12 + 16;

/// The 32-byte key the wrapper used, one of the store's at-rest keys.
pub(super) struct WrapperKey<'a>(&'a [u8; 32]);

impl<'a> WrapperKey<'a> {
    /// The first of `keys` under which `value`, stored at `key`, opens.
    pub(super) fn find<I>(keys: I, key: &[u8], value: &[u8]) -> Option<Self>
    where
        I: IntoIterator<Item = &'a [u8; 32]>,
    {
        keys.into_iter()
            .map(WrapperKey)
            .find(|candidate| candidate.open(key, value).is_some())
    }

    /// The plaintext of `value` stored at `key`, or `None` when the tag does
    /// not verify: a wrong key, a value from another key, or bytes the wrapper
    /// did not write.
    pub(super) fn open(&self, key: &[u8], value: &[u8]) -> Option<Vec<u8>> {
        crypto::decrypt_aes(None, value, self.0, key).ok()
    }
}

#[cfg(test)]
pub(super) fn seal(key: &[u8; 32], storage_key: &[u8], value: &[u8]) -> Vec<u8> {
    crypto::encrypt_aes(value, key, storage_key, true)
        .map(|(sealed, _)| sealed)
        .expect("test encryption")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_only_under_its_key_and_its_storage_key() {
        let sealed = seal(&[1; 32], b"k", b"value");
        assert_eq!(sealed.len(), b"value".len() + OVERHEAD);
        let keys = [[2u8; 32], [1; 32]];
        let found = WrapperKey::find(&keys, b"k", &sealed).unwrap();
        assert_eq!(found.open(b"k", &sealed).as_deref(), Some(&b"value"[..]));
        assert!(found.open(b"other", &sealed).is_none());
        assert!(WrapperKey::find(&keys[..1], b"k", &sealed).is_none());
        assert!(found.open(b"k", b"value written in plain").is_none());
    }
}
