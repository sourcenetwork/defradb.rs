//! [`VectorNodeStore`] over a transaction.
//!
//! Borrows the transaction rather than owning it, so every write lands in the
//! caller's transaction. Traversal is advisory, additions are operands, and
//! structural replacements retain validated reads.

use std::ops::ControlFlow;

use bytes::Bytes;
use defra_core::thread_bounds::MaybeSend;
use storage::corekv::AppendLog;
use storage::corekv::{IterOptions, Key, Reader, Writer};
use storage::keys::datastore::{
    vector_epoch_prefix, vector_pending_log, VectorAuxKey, VectorIndexKey,
};

use super::{Meta, Node, NodeId, VectorNodeStore};
use crate::index::error::Result;
use crate::index::vector::codec::{decode_meta, decode_node, encode_meta, encode_node};

/// One epoch of one vector index, over a transaction.
#[derive(Debug)]
pub struct KvNodeStore<'txn, T> {
    txn: &'txn mut T,
    collection_short_id: u32,
    index_id: u32,
    epoch: u32,
}

impl<'txn, T> KvNodeStore<'txn, T> {
    pub fn new(txn: &'txn mut T, collection_short_id: u32, index_id: u32, epoch: u32) -> Self {
        Self {
            txn,
            collection_short_id,
            index_id,
            epoch,
        }
    }

    /// Which build of the index this reads and writes.
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    fn aux_key(&self, kind: u8, key: &[u8]) -> Vec<u8> {
        VectorAuxKey::new(
            self.collection_short_id,
            self.index_id,
            self.epoch,
            kind,
            key,
        )
        .bytes()
    }

    fn node_key(&self, id: NodeId) -> Vec<u8> {
        VectorIndexKey::node(self.collection_short_id, self.index_id, self.epoch, id.0).bytes()
    }

    fn meta_key(&self) -> Vec<u8> {
        VectorIndexKey::meta(self.collection_short_id, self.index_id, self.epoch).bytes()
    }

    fn node_prefix(&self) -> Vec<u8> {
        VectorIndexKey::node_prefix(self.collection_short_id, self.index_id, self.epoch).bytes()
    }

    fn pending_log_keys(&self) -> AppendLog {
        vector_pending_log(self.collection_short_id, self.index_id, self.epoch)
    }

    /// Every key of this build: nodes, meta, and every aux kind beside them.
    fn epoch_prefix(&self) -> Vec<u8> {
        vector_epoch_prefix(self.collection_short_id, self.index_id, self.epoch)
    }
}

/// The iterator is reopened per batch because deleting through a live one is
/// not defined for every backend.
const CLEAR_BATCH: usize = 1024;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T: Reader + Writer + MaybeSend> VectorNodeStore for KvNodeStore<'_, T> {
    async fn get_node(&self, id: NodeId) -> Result<Option<Node>> {
        match self.txn.get(&self.node_key(id)).await? {
            Some(bytes) => Ok(Some(decode_node(&bytes)?)),
            None => Ok(None),
        }
    }

    async fn get_node_advisory(&self, id: NodeId) -> Result<Option<Node>> {
        self.txn
            .get_parts(&self.node_key(id), &[])
            .await?
            .map(|bytes| decode_node(&bytes))
            .transpose()
    }

    async fn get_node_for_links(&self, id: NodeId) -> Result<Option<Node>> {
        self.txn
            .get_parts(&self.node_key(id), &[0])
            .await?
            .map(|bytes| decode_node(&bytes))
            .transpose()
    }

    async fn add_link(&mut self, id: NodeId, layer: usize, neighbor: NodeId) -> Result<()> {
        let layer = u32::try_from(layer)
            .map_err(|_| crate::index::error::Error::Other("link layer overflow".into()))?;
        self.txn
            .merge(
                &self.node_key(id),
                &storage::merge::vector::links(layer, neighbor.0),
            )
            .await?;
        Ok(())
    }

    async fn put_node(&mut self, node: Node) -> Result<()> {
        let key = self.node_key(node.id);
        self.txn.set(&key, &encode_node(&node)).await?;
        Ok(())
    }

    async fn get_meta(&self) -> Result<Option<Meta>> {
        match self.txn.get(&self.meta_key()).await? {
            Some(bytes) => Ok(Some(decode_meta(&bytes)?)),
            None => Ok(None),
        }
    }

    async fn get_meta_advisory(&self) -> Result<Option<Meta>> {
        match self.txn.get_parts(&self.meta_key(), &[]).await? {
            Some(bytes) => Ok(Some(decode_meta(&bytes)?)),
            // Creating the first entry point is an ordinary one-winner decision.
            None => self.get_meta().await,
        }
    }

    async fn add_counts(&mut self, live: i64, waste: u64) -> Result<()> {
        self.txn
            .merge(
                &self.meta_key(),
                &storage::merge::vector::counts(live, waste),
            )
            .await?;
        Ok(())
    }

    async fn put_meta(&mut self, meta: Meta) -> Result<()> {
        let key = self.meta_key();
        self.txn.set(&key, &encode_meta(&meta)).await?;
        Ok(())
    }

    /// Streams the epoch's nodes, holding one at a time rather than the graph.
    /// The prefix stops at this epoch's node discriminator.
    async fn iterate_nodes<F>(&self, mut visit: F) -> Result<()>
    where
        F: FnMut(Node) -> Result<()> + MaybeSend,
    {
        let mut iter = self
            .txn
            .iterator(IterOptions::default().with_prefix(self.node_prefix()))
            .await?;
        while let Some(pair) = iter.next().await? {
            let node = decode_node(&pair.value)?;
            if node.deleted {
                continue;
            }
            visit(node)?;
        }
        Ok(())
    }

    /// Removes every key of this epoch, in batches, so the memory held is
    /// bounded by the batch size rather than by the number of nodes.
    ///
    /// The scan is over the whole epoch prefix rather than the node prefix,
    /// which is what makes "every key" true: the aux space holds a partitioned
    /// kind's centroids, codebooks and inverted lists, and a graph kind's
    /// adjacency, and none of it is under the node prefix. Leaving it behind
    /// corrupts a reindex, because a surviving trained-state marker makes the
    /// engine append to lists built from centroids that are gone, and leaks the
    /// whole aux keyspace on a drop, because nothing else ever removes it.
    ///
    /// Prefix rather than a per-kind sweep so a kind added later is covered
    /// without anyone remembering to add it here.
    ///
    /// The pending log keeps its head, which regolith never rewinds; only its
    /// entries go.
    async fn clear(&mut self) -> Result<()> {
        loop {
            let mut batch = Vec::new();
            {
                let mut iter = self
                    .txn
                    .iterator(IterOptions::default().with_prefix(self.epoch_prefix()))
                    .await?;
                while let Some(pair) = iter.next().await? {
                    batch.push(pair.key);
                    if batch.len() >= CLEAR_BATCH {
                        break;
                    }
                }
            }
            if batch.is_empty() {
                break;
            }
            for key in batch {
                self.txn.delete(&key).await?;
            }
        }
        if let Some(tail) = self.pending_log_tail().await? {
            let head = self.pending_log_head().await?;
            if tail <= head {
                self.truncate_pending_log(tail, head).await?;
            }
        }
        Ok(())
    }

    async fn get_aux(&self, kind: u8, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.txn.get(&self.aux_key(kind, key)).await?)
    }

    async fn put_aux(&mut self, kind: u8, key: &[u8], value: &[u8]) -> Result<()> {
        let full = self.aux_key(kind, key);
        self.txn.set(&full, value).await?;
        Ok(())
    }

    async fn delete_aux(&mut self, kind: u8, key: &[u8]) -> Result<()> {
        let full = self.aux_key(kind, key);
        self.txn.delete(&full).await?;
        Ok(())
    }

    async fn write_aux_from_nodes<F>(&mut self, kind: u8, mut encode: F) -> Result<u64>
    where
        F: FnMut(Node) -> Result<(Vec<u8>, Vec<u8>)> + MaybeSend,
    {
        let mut iter = self
            .txn
            .iterator(IterOptions::default().with_prefix(self.node_prefix()))
            .await?;
        let mut count = 0;
        while let Some(pair) = iter.next().await? {
            let node = decode_node(&pair.value)?;
            if node.deleted {
                continue;
            }
            let (key, value) = encode(node)?;
            self.txn.set(&self.aux_key(kind, &key), &value).await?;
            count += 1;
        }
        Ok(count)
    }

    /// Streams one kind's entries, holding one at a time. The prefix stops at
    /// this epoch's discriminator, so a scan cannot reach the graph or another
    /// kind.
    async fn iterate_aux<F>(&self, kind: u8, key_prefix: &[u8], mut visit: F) -> Result<()>
    where
        F: FnMut(&[u8], &[u8]) -> Result<()> + MaybeSend,
    {
        let prefix = self.aux_key(kind, key_prefix);
        // Every stored entry has a separator after the discriminator, whether
        // or not the scan narrowed by a prefix.
        let strip = self.aux_key(kind, &[]).len() + 1;
        let mut iter = self
            .txn
            .iterator(IterOptions::default().with_prefix(prefix))
            .await?;
        while let Some(pair) = iter.next().await? {
            let key = pair.key.get(strip..).unwrap_or_default();
            visit(key, &pair.value)?;
        }
        Ok(())
    }

    /// The head is read like any log key, never validated.
    async fn pending_log_head(&self) -> Result<u64> {
        self.txn
            .get(self.pending_log_keys().head_key())
            .await?
            .map_or(Ok(0), |bytes| be_u64(&bytes))
    }

    async fn append_pending_log(&mut self, entry: &[u8]) -> Result<()> {
        let log = self.pending_log_keys();
        self.txn.append(&log, entry, None).await?;
        Ok(())
    }

    async fn own_pending_log(&self) -> Result<Vec<Vec<u8>>> {
        Ok(self.txn.appended(&self.pending_log_keys()).await?)
    }

    async fn pending_log_entry(&self, position: u64) -> Result<Option<Bytes>> {
        Ok(self
            .txn
            .get(&self.pending_log_keys().entry_key(position))
            .await?)
    }

    /// Entry keys sort by position (zero-padded decimal), so a prefix scan
    /// visits the oldest entry held first.
    async fn iterate_pending_log<F>(&self, mut visit: F) -> Result<()>
    where
        F: FnMut(u64, &[u8]) -> Result<ControlFlow<()>> + MaybeSend,
    {
        let log = self.pending_log_keys();
        let mut iter = self
            .txn
            .iterator(IterOptions::default().with_prefix(log.entry_prefix().to_vec()))
            .await?;
        while let Some(pair) = iter.next().await? {
            let position = entry_position(&log, &pair.key)?;
            if visit(position, &pair.value)?.is_break() {
                break;
            }
        }
        Ok(())
    }

    async fn truncate_pending_log(&mut self, from: u64, through: u64) -> Result<()> {
        let log = self.pending_log_keys();
        self.txn.truncate_log(&log, from..=through).await?;
        Ok(())
    }

    async fn pending_log_tail(&self) -> Result<Option<u64>> {
        let mut tail = None;
        self.iterate_pending_log(|position, _| {
            tail = Some(position);
            Ok(ControlFlow::Break(()))
        })
        .await?;
        Ok(tail)
    }
}

/// The position an entry key names.
fn entry_position(log: &AppendLog, key: &[u8]) -> Result<u64> {
    let digits = key.get(log.entry_prefix().len()..).unwrap_or_default();
    std::str::from_utf8(digits)
        .ok()
        .and_then(|digits| digits.parse().ok())
        .ok_or_else(|| {
            crate::index::error::Error::Other("pending log entry key is not a position".into())
        })
}

/// A count or a log head: eight big-endian bytes.
fn be_u64(bytes: &[u8]) -> Result<u64> {
    bytes.try_into().map(u64::from_be_bytes).map_err(|_| {
        crate::index::error::Error::Other("pending log number is not eight bytes".into())
    })
}
