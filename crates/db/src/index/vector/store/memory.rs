//! An in-memory [`VectorNodeStore`].
//!
//! Not a test fixture: it is the store an ephemeral or embedded index uses, and
//! it is what lets the engine be exercised with no database. A `BTreeMap` keeps
//! iteration order stable, so a brute-force baseline built from it is
//! reproducible.

use bytes::Bytes;
use std::collections::BTreeMap;
use std::ops::ControlFlow;

use defra_core::thread_bounds::MaybeSend;

use super::{Meta, Node, NodeId, VectorNodeStore};
use crate::index::error::Result;

/// A graph held entirely in memory.
#[derive(Debug, Default, Clone)]
pub struct MemoryNodeStore {
    nodes: BTreeMap<NodeId, Node>,
    meta: Option<Meta>,
    aux: BTreeMap<(u8, Vec<u8>), Bytes>,
    /// The pending log's entries still held, by position.
    log: BTreeMap<u64, Bytes>,
    /// The newest position the log has given out.
    head: u64,
}

impl MemoryNodeStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Nodes held, tombstones included.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl VectorNodeStore for MemoryNodeStore {
    async fn get_node(&self, id: NodeId) -> Result<Option<Node>> {
        Ok(self.nodes.get(&id).cloned())
    }

    async fn put_node(&mut self, node: Node) -> Result<()> {
        self.nodes.insert(node.id, node);
        Ok(())
    }

    async fn get_meta(&self) -> Result<Option<Meta>> {
        Ok(self.meta)
    }

    async fn put_meta(&mut self, meta: Meta) -> Result<()> {
        self.meta = Some(meta);
        Ok(())
    }

    async fn iterate_nodes<F>(&self, mut visit: F) -> Result<()>
    where
        F: FnMut(Node) -> Result<()> + MaybeSend,
    {
        for node in self.nodes.values() {
            if node.deleted {
                continue;
            }
            visit(node.clone())?;
        }
        Ok(())
    }

    async fn clear(&mut self) -> Result<()> {
        self.nodes.clear();
        self.meta = None;
        self.aux.clear();
        self.log.clear();
        Ok(())
    }

    async fn get_aux(&self, kind: u8, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.aux.get(&(kind, key.to_vec())).cloned())
    }

    async fn put_aux(&mut self, kind: u8, key: &[u8], value: &[u8]) -> Result<()> {
        self.aux
            .insert((kind, key.to_vec()), Bytes::copy_from_slice(value));
        Ok(())
    }

    async fn delete_aux(&mut self, kind: u8, key: &[u8]) -> Result<()> {
        self.aux.remove(&(kind, key.to_vec()));
        Ok(())
    }

    async fn write_aux_from_nodes<F>(&mut self, kind: u8, mut encode: F) -> Result<u64>
    where
        F: FnMut(Node) -> Result<(Vec<u8>, Vec<u8>)> + MaybeSend,
    {
        let mut count = 0;
        for node in self.nodes.values().filter(|node| !node.deleted) {
            let (key, value) = encode(node.clone())?;
            self.aux.insert((kind, key), Bytes::from(value));
            count += 1;
        }
        Ok(count)
    }

    /// A range over the ordered map, not a full scan: an inverted-list probe
    /// is a prefix lookup and must cost what the prefix holds, not what the
    /// index holds.
    async fn iterate_aux<F>(&self, kind: u8, key_prefix: &[u8], mut visit: F) -> Result<()>
    where
        F: FnMut(&[u8], &[u8]) -> Result<()> + MaybeSend,
    {
        let start = (kind, key_prefix.to_vec());
        for ((entry_kind, key), value) in self.aux.range(start..) {
            if *entry_kind != kind || !key.starts_with(key_prefix) {
                break;
            }
            visit(key, value)?;
        }
        Ok(())
    }

    /// One writer and no transactions: every append is committed at once, so
    /// a store never has appends of its own to hand back.
    async fn pending_log_head(&self) -> Result<u64> {
        Ok(self.head)
    }

    async fn append_pending_log(&mut self, entry: &[u8]) -> Result<()> {
        self.head += 1;
        self.log.insert(self.head, Bytes::copy_from_slice(entry));
        Ok(())
    }

    async fn pending_log_entry(&self, position: u64) -> Result<Option<Bytes>> {
        Ok(self.log.get(&position).cloned())
    }

    async fn iterate_pending_log<F>(&self, mut visit: F) -> Result<()>
    where
        F: FnMut(u64, &[u8]) -> Result<ControlFlow<()>> + MaybeSend,
    {
        for (position, entry) in &self.log {
            if visit(*position, entry)?.is_break() {
                break;
            }
        }
        Ok(())
    }

    async fn truncate_pending_log(&mut self, from: u64, through: u64) -> Result<()> {
        if from <= through {
            self.log
                .retain(|position, _| !(from..=through).contains(position));
        }
        Ok(())
    }

    async fn pending_log_tail(&self) -> Result<Option<u64>> {
        Ok(self.log.keys().next().copied())
    }
}
