//! The persistence port, mirroring Go's `hnsw.NodeStore`.
//!
//! The engine never imports a concrete store, so it is testable with no
//! database at all. Any medium can back this: the in-memory store here, or a
//! transactional KV adapter.

pub mod memory;

pub use memory::MemoryNodeStore;

use bytes::Bytes;
use defra_core::thread_bounds::{MaybeSend, MaybeSendSync};

use crate::index::error::Result;

/// Identifies one vector in the graph.
///
/// The adapter maps its own identifiers (a document short id) onto this; the
/// engine attaches no meaning to the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

/// The persisted form of a graph node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: NodeId,
    /// Normalized under the cosine metric, which is the only one the query
    /// surface exposes.
    pub vector: Vec<f32>,
    /// `layers[l]` holds the neighbor ids at layer `l`, so `layers.len()` is
    /// the node's top layer plus one.
    pub layers: Vec<Vec<NodeId>>,
    /// Tombstone. The node stays linked so traversal through it is preserved,
    /// but it is never returned from a search.
    pub deleted: bool,
}

impl Node {
    /// A node of the given height with no links yet.
    pub fn new(id: NodeId, vector: Vec<f32>, top_layer: usize) -> Self {
        Self {
            id,
            vector,
            layers: vec![Vec::new(); top_layer + 1],
            deleted: false,
        }
    }

    /// Neighbors at `layer`, empty when the node does not reach that high.
    pub fn neighbors(&self, layer: usize) -> &[NodeId] {
        self.layers.get(layer).map_or(&[], Vec::as_slice)
    }
}

/// The graph's global state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meta {
    /// Where every search starts.
    pub entry_point: NodeId,
    /// Highest layer any node currently occupies.
    pub top_layer: usize,
    /// Live nodes counted since the store began. Approximate bookkeeping for
    /// the rebuild trigger, not a queryable census: a meta decoded from an
    /// older layout reads zero here until the next rebuild recounts.
    pub live: u64,
    /// Tombstones since the last rebuild. The unit the rebuild trigger is
    /// priced in: one tombstone, or one stale link a rebuild would drop.
    pub waste: u64,
    /// Rebuilds this build of the graph has been through. Zero on a graph
    /// last written before self-link-free inserts, which marks it as owed a
    /// healing rebuild; a graph created by the current code starts at one.
    pub rebuilds: u32,
}

/// Where a graph lives.
///
/// Generic rather than `dyn`, matching `CollectionIndex`: the KV adapter is
/// parameterised over the transaction type, which a trait object cannot carry.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait VectorNodeStore: MaybeSendSync {
    /// `None` when no such node has been stored.
    async fn get_node(&self, id: NodeId) -> Result<Option<Node>>;

    /// Stores `node`, replacing any node with the same id.
    async fn put_node(&mut self, node: Node) -> Result<()>;

    /// `None` when no graph has been built yet, so there is no entry point to
    /// start from.
    async fn get_meta(&self) -> Result<Option<Meta>>;

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
}
