use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use bitswap::{Block, Store};
use cid::Cid;

#[derive(Debug, Clone)]
pub struct MemStore(Arc<BTreeMap<Cid, Block>>);

impl MemStore {
    pub fn new(blocks: &[Block]) -> Self {
        MemStore(Arc::new(
            blocks.iter().map(|b| (b.cid, b.clone())).collect(),
        ))
    }
}

#[async_trait]
impl Store for MemStore {
    async fn get_size(&self, cid: &Cid) -> anyhow::Result<usize> {
        self.0
            .get(cid)
            .map(|b| b.data.len())
            .ok_or_else(|| anyhow!("not found"))
    }

    async fn get(&self, cid: &Cid) -> anyhow::Result<Block> {
        self.0.get(cid).cloned().ok_or_else(|| anyhow!("not found"))
    }

    async fn has(&self, cid: &Cid) -> anyhow::Result<bool> {
        Ok(self.0.contains_key(cid))
    }
}
