//! The local key tier: verified keys in memory, bounded by bytes, in front
//! of the persistent [`KeyStore`].
//!
//! A key's bytes are content-addressed by its `Encryption` block's CID and
//! never change, so a key verified once is exact forever: the memory tier
//! needs no time to live, only a byte bound. What it holds is the key, never
//! a decision to release it: every caller's release is decided again.

use std::sync::Arc;

use crate::error::{Error, Result};
use crate::store::{KeyStore, StoredKey};
use crate::types::EncryptionCid;

/// What one cached key is charged beyond its block bytes: the key, its CID
/// and the `Arc`.
const KEY_BYTES: usize = 32 + 64 + 16;

pub(crate) struct KeyTier {
    store: Arc<dyn KeyStore>,
    memory: inputs::Cache<EncryptionCid, Arc<StoredKey>>,
}

impl KeyTier {
    pub(crate) fn new(store: Arc<dyn KeyStore>, memory_bytes: usize) -> Self {
        Self {
            store,
            memory: inputs::Cache::new(memory_bytes, None),
        }
    }

    pub(crate) fn store(&self) -> &Arc<dyn KeyStore> {
        &self.store
    }

    /// The verified key `cid` names, from memory or else the store. A stored
    /// block that fails verification is reported and treated as absent, so
    /// the key is fetched again.
    pub(crate) async fn get(&self, cid: &EncryptionCid) -> Result<Option<Arc<StoredKey>>> {
        if let Some(stored) = self.memory.get(cid, 0) {
            return Ok(Some(stored));
        }
        let Some(stored) = self.store.get(cid).await? else {
            return Ok(None);
        };
        if let Err(error) = verified_key(cid, &stored.block_bytes) {
            tracing::warn!(
                cid = %cid,
                error = %error,
                "Ignoring invalid locally stored encryption block"
            );
            return Ok(None);
        }
        let stored = Arc::new(stored);
        self.remember(*cid, Arc::clone(&stored));
        Ok(Some(stored))
    }

    /// Persists a verified key and keeps it in memory.
    pub(crate) async fn put(&self, cid: EncryptionCid, stored: StoredKey) -> Result<()> {
        self.store.put(cid, stored.clone()).await?;
        self.remember(cid, Arc::new(stored));
        Ok(())
    }

    /// Keeps a key this node holds (one it generated or verified) in memory.
    pub(crate) fn remember(&self, cid: EncryptionCid, stored: Arc<StoredKey>) {
        let bytes = KEY_BYTES + stored.block_bytes.len();
        let generation = self.memory.generation();
        self.memory.insert(cid, stored, bytes, 0, generation);
    }
}

/// The key an `Encryption` block carries, once its bytes hash to `cid`.
pub(crate) fn verified_key(cid: &EncryptionCid, block_bytes: &[u8]) -> Result<[u8; 32]> {
    let block = defra_core::Encryption::from_dag_cbor(block_bytes)
        .map_err(|error| Error::Crypto(format!("decode encryption block: {error}")))?;
    let computed_cid = block
        .generate_cid()
        .map_err(|error| Error::Crypto(format!("compute encryption block CID: {error}")))?;
    if computed_cid != *cid {
        return Err(Error::Crypto(format!(
            "encryption block CID mismatch: expected {cid}, got {computed_cid}"
        )));
    }
    block.key.as_slice().try_into().map_err(|_| {
        Error::Crypto(format!(
            "encryption block {cid} has invalid key length {}",
            block.key.len()
        ))
    })
}
