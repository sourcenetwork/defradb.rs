//! The persistence port, mirroring Go's `hnsw.NodeStore`.
//!
//! The engine never imports a concrete store, so it is testable with no
//! database at all. Any medium can back this: the in-memory store here, or a
//! transactional KV adapter.

pub mod kv;
pub mod memory;

pub use kv::KvNodeStore;
pub use memory::MemoryNodeStore;

use std::ops::ControlFlow;

use bytes::Bytes;
use defra_core::thread_bounds::{MaybeSend, MaybeSendSync};

use crate::index::error::Result;

pub use storage::vector::{Meta, Node, NodeId};

/// Where a graph lives.
///
/// Generic rather than `dyn`, matching `CollectionIndex`: the KV adapter is
/// parameterised over the transaction type, which a trait object cannot carry.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait VectorNodeStore: MaybeSendSync {
    /// `None` when no such node has been stored.
    async fn get_node(&self, id: NodeId) -> Result<Option<Node>>;

    /// Advisory graph traversal; no decision may depend on adjacency remaining unchanged.
    async fn get_node_advisory(&self, id: NodeId) -> Result<Option<Node>> {
        self.get_node(id).await
    }

    /// Validate node structure while allowing concurrent adjacency additions.
    async fn get_node_for_links(&self, id: NodeId) -> Result<Option<Node>> {
        self.get_node(id).await
    }

    async fn add_link(&mut self, id: NodeId, layer: usize, neighbor: NodeId) -> Result<()> {
        if let Some(mut node) = self.get_node_for_links(id).await? {
            let links = node
                .layers
                .get_mut(layer)
                .ok_or_else(|| crate::index::error::Error::Other("link layer is absent".into()))?;
            links.retain(|link| *link != id);
            if neighbor != id {
                links.push(neighbor);
            }
            links.sort_unstable();
            links.dedup();
            self.put_node(node).await?;
        }
        Ok(())
    }

    /// Stores `node`, replacing any node with the same id.
    async fn put_node(&mut self, node: Node) -> Result<()>;

    /// `None` when no graph has been built yet, so there is no entry point to
    /// start from.
    async fn get_meta(&self) -> Result<Option<Meta>>;

    /// Advisory routing metadata; a structural replacement must re-read with get_meta.
    async fn get_meta_advisory(&self) -> Result<Option<Meta>> {
        self.get_meta().await
    }

    async fn add_counts(&mut self, live: i64, waste: u64) -> Result<()> {
        if let Some(mut meta) = self.get_meta().await? {
            meta.live = meta.live.saturating_add_signed(live);
            meta.waste = meta.waste.saturating_add(waste);
            self.put_meta(meta).await?;
        }
        Ok(())
    }

    /// Stores `meta`, replacing any previous value.
    async fn put_meta(&mut self, meta: Meta) -> Result<()>;

    /// Visits every **non-deleted** node, in unspecified order, stopping early
    /// if `visit` returns an error. Used for brute-force baselines and, later,
    /// for rebuilding an epoch.
    async fn iterate_nodes<F>(&self, visit: F) -> Result<()>
    where
        F: FnMut(Node) -> Result<()> + MaybeSend;

    /// Removes every key of this build: nodes, meta, and every aux kind
    /// beside them. What a rebuild starts from, and what an index drop
    /// leaves behind.
    async fn clear(&mut self) -> Result<()>;

    /// A namespaced blob space private to this index and epoch, for whatever a
    /// kind needs beyond nodes: coarse centroids, codebooks, inverted lists.
    ///
    /// `kind` separates concepts and `key` is the kind's own encoding, so a
    /// kind adds a concept without a port change. Graph-only kinds never call
    /// these.
    async fn get_aux(&self, kind: u8, key: &[u8]) -> Result<Option<Bytes>>;

    async fn put_aux(&mut self, kind: u8, key: &[u8], value: &[u8]) -> Result<()>;

    /// Map each live node to one auxiliary entry, writing it before visiting
    /// the next node. Holds one node and one output entry at a time.
    async fn write_aux_from_nodes<F>(&mut self, kind: u8, encode: F) -> Result<u64>
    where
        F: FnMut(Node) -> Result<(Vec<u8>, Vec<u8>)> + MaybeSend;

    /// Removes one entry, if it is there. Absent is not an error: a caller
    /// clearing an entry it is not sure exists is the normal case.
    ///
    /// Without this a partitioned kind can only ever add, so a vector that
    /// moves between lists leaves its old entry behind to be found by a later
    /// probe.
    async fn delete_aux(&mut self, kind: u8, key: &[u8]) -> Result<()>;

    /// Visits every entry of `kind` whose key starts with `key_prefix`, in key
    /// order, one at a time.
    async fn iterate_aux<F>(&self, kind: u8, key_prefix: &[u8], visit: F) -> Result<()>
    where
        F: FnMut(&[u8], &[u8]) -> Result<()> + MaybeSend;

    /// The newest committed position of this build's pending log in the
    /// transaction's snapshot; 0 before the first append.
    async fn pending_log_head(&self) -> Result<u64>;

    /// Adds `entry` to the pending log, at the position its commit takes.
    async fn append_pending_log(&mut self, entry: &[u8]) -> Result<()>;

    /// The entries this transaction appended and has not committed, oldest
    /// first. No log read shows them, since they take a position only at
    /// commit.
    async fn own_pending_log(&self) -> Result<Vec<Vec<u8>>> {
        Ok(Vec::new())
    }

    /// The entry at `position`; `None` once truncated.
    async fn pending_log_entry(&self, position: u64) -> Result<Option<Bytes>>;

    /// Visits every entry still held, oldest first, with its position, until
    /// `visit` breaks.
    async fn iterate_pending_log<F>(&self, visit: F) -> Result<()>
    where
        F: FnMut(u64, &[u8]) -> Result<ControlFlow<()>> + MaybeSend;

    /// Drops every entry from `from` through `through`, which a merge has
    /// absorbed. The head stays, so positions keep counting.
    async fn truncate_pending_log(&mut self, from: u64, through: u64) -> Result<()>;

    /// The oldest entry still held, `None` when every entry was truncated.
    async fn pending_log_tail(&self) -> Result<Option<u64>>;
}
