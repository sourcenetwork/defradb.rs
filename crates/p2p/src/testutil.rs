//! Test utilities for P2P crate.
//!
//! This module provides common test helpers and mocks used across unit and integration tests.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use iroh_bitswap::{Block, Store};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

/// Mock BitswapStore for testing.
///
/// A simple in-memory implementation of iroh_bitswap::Store that can be used
/// in unit and integration tests without requiring a real blockstore.
#[derive(Clone, Debug)]
pub struct MockBitswapStore {
    blocks: Arc<Mutex<HashMap<Cid, Vec<u8>>>>,
}

impl MockBitswapStore {
    /// Create a new empty mock store.
    pub fn new() -> Self {
        Self {
            blocks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Pre-populate the store with a block.
    pub fn with_block(self, cid: Cid, data: Vec<u8>) -> Self {
        self.blocks.lock().unwrap().insert(cid, data);
        self
    }

    /// Get the number of blocks in the store.
    pub fn len(&self) -> usize {
        self.blocks.lock().unwrap().len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.blocks.lock().unwrap().is_empty()
    }
}

impl Default for MockBitswapStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Store for MockBitswapStore {
    async fn get_size(&self, cid: &Cid) -> Result<usize> {
        self.blocks
            .lock()
            .unwrap()
            .get(cid)
            .map(|data| data.len())
            .ok_or_else(|| anyhow!("block not found: {}", cid))
    }

    async fn get(&self, cid: &Cid) -> Result<Block> {
        let data = self
            .blocks
            .lock()
            .unwrap()
            .get(cid)
            .cloned()
            .ok_or_else(|| anyhow!("block not found: {}", cid))?;
        Ok(Block::new(Bytes::from(data), *cid))
    }

    async fn has(&self, cid: &Cid) -> Result<bool> {
        Ok(self.blocks.lock().unwrap().contains_key(cid))
    }
}

/// Milliseconds to stall between spawning a Bitswap fetch task and registering
/// its query. Tests set this to force the fetch to finish first, which is the
/// interleaving that used to leave a completed query registered forever.
pub static BITSWAP_REGISTER_STALL_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Stalls for [`BITSWAP_REGISTER_STALL_MS`], yielding to the runtime so the
/// just-spawned fetch task is free to run to completion meanwhile.
pub async fn stall_before_query_registration() {
    let ms = BITSWAP_REGISTER_STALL_MS.load(std::sync::atomic::Ordering::Relaxed);
    if ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }
}

/// Milliseconds a started Bitswap fetch task blocks its worker thread while
/// holding its `Session` clone. An `abort` cannot land while a task is off an
/// await point, so tests set this to force the interleaving where a cancel's
/// `stop_session` runs against a still-live clone.
pub static BITSWAP_FETCH_BLOCK_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Blocks the current worker thread for [`BITSWAP_FETCH_BLOCK_MS`].
pub fn block_started_fetch() {
    let ms = BITSWAP_FETCH_BLOCK_MS.load(std::sync::atomic::Ordering::Relaxed);
    if ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_store_basic_operations() {
        let store = MockBitswapStore::new();
        assert!(store.is_empty());

        // Create a test CID
        let cid =
            Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap();

        // Check not present
        assert!(!store.has(&cid).await.unwrap());

        // Get missing returns error
        assert!(store.get(&cid).await.is_err());
    }

    #[test]
    fn test_mock_store_with_block() {
        let cid =
            Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap();
        let store = MockBitswapStore::new().with_block(cid, b"test data".to_vec());

        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }
}
