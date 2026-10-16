//! The block source the server reads from.

use std::fmt::Debug;

use anyhow::Result;
use async_trait::async_trait;
use cid::Cid;

use crate::block::Block;

/// Read access to a block store; any error means the block is absent.
#[async_trait]
pub trait Store: Debug + Clone + Send + Sync + 'static {
    /// Size in bytes of the block.
    async fn get_size(&self, cid: &Cid) -> Result<usize>;
    /// The block itself.
    async fn get(&self, cid: &Cid) -> Result<Block>;
    /// Whether the block is present.
    async fn has(&self, cid: &Cid) -> Result<bool>;
}
