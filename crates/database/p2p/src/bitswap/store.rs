//! BitswapStore adapter for DefraBlockstore.
//!
//! This module implements the `Store` trait from the bitswap crate
//! for our async `DefraBlockstore`. The trait methods are async, which maps
//! directly to our async blockstore interface.
//!
//! # Go Compatibility
//!
//! Go DefraDB uses Bitswap for block exchange. This adapter enables the same
//! block exchange protocol in Rust, ensuring interoperability.

use std::fmt::Debug;
use std::sync::Arc;

use ::bitswap::Block;
use cid::Cid;

use blockstore::Blockstore;

use super::store_trait::impl_bitswap_store;
use crate::{Error, Result};

/// Adapter that implements bitswap::Store for any `Blockstore`.
///
/// The Store trait from the bitswap crate uses async methods,
/// which maps directly to our async DefraBlockstore interface.
///
/// # Thread Safety
///
/// This adapter is thread-safe and can be cloned. The underlying blockstore
/// is shared via `Arc`.
#[derive(Debug)]
pub struct BitswapStoreAdapter<B: Blockstore> {
    blockstore: Arc<B>,
}

impl<B: Blockstore> Clone for BitswapStoreAdapter<B> {
    fn clone(&self) -> Self {
        Self {
            blockstore: self.blockstore.clone(),
        }
    }
}

impl<B: Blockstore> BitswapStoreAdapter<B> {
    /// Create a new BitswapStore adapter.
    ///
    /// # Arguments
    ///
    /// * `blockstore` - The underlying async blockstore
    pub fn new(blockstore: Arc<B>) -> Self {
        Self { blockstore }
    }
}

impl<B: Blockstore + Debug + 'static> BitswapStoreAdapter<B> {
    async fn block_size(&self, cid: &Cid) -> Result<usize> {
        self.blockstore
            .get_size(cid)
            .await
            .map_err(|e| Error::BlockstoreError(e.to_string()))?
            .ok_or_else(|| Error::BlockNotFound(cid.to_string()))
    }

    async fn block(&self, cid: &Cid) -> Result<Block> {
        let data = self
            .blockstore
            .get(cid)
            .await
            .map_err(|e| Error::BlockstoreError(e.to_string()))?
            .ok_or_else(|| Error::BlockNotFound(cid.to_string()))?;
        Ok(Block::new(data, *cid))
    }

    async fn contains(&self, cid: &Cid) -> Result<bool> {
        self.blockstore
            .has(cid)
            .await
            .map_err(|e| Error::BlockstoreError(e.to_string()))
    }
}

impl_bitswap_store!([B: Blockstore + Debug + 'static] BitswapStoreAdapter<B>);
