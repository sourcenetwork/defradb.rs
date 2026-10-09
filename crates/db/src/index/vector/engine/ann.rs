//! What every approximate-nearest-neighbor index kind has in common.
//!
//! The kinds themselves are siblings of this module. Nothing here knows about
//! any of them, which is the point: a new kind adds a sibling module and
//! implements [`VectorIndexEngine`], and the store port, the key layout and the
//! `CollectionIndex` wiring are untouched.

use std::cmp::Ordering;
use std::sync::Arc;

use defra_core::thread_bounds::MaybeSendSync;
use defra_core::vector::Element;

use crate::index::error::Result;
use crate::index::vector::store::NodeId;

/// Decides which nodes a search may *return*. A rejected node is still walked
/// through, exactly as a tombstone is: excluding it from the walk would strand
/// whatever lies behind it.
pub trait Admit: MaybeSendSync {
    fn admits(&self, id: NodeId) -> bool;
}

/// Every node qualifies.
#[derive(Debug, Clone, Copy)]
pub struct AdmitAll;

impl Admit for AdmitAll {
    fn admits(&self, _id: NodeId) -> bool {
        true
    }
}

impl<F: Fn(NodeId) -> bool + MaybeSendSync> Admit for F {
    fn admits(&self, id: NodeId) -> bool {
        self(id)
    }
}

/// One vector index kind.
///
/// Every method speaks only of a node id and a vector, so a kind never sees a
/// document, a transaction or a collection, and stays testable with no
/// database. Same idea as pulsejetdb's `ANNIndex` but not its shape, which is
/// build-then-query, in-memory, and panics on a dimension mismatch.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait VectorIndexEngine: MaybeSendSync {
    /// Which algorithm this is.
    fn kind(&self) -> EngineKind;

    /// Adds `vector` under `id`, replacing any vector already there.
    ///
    /// Generic over the element width because a vector reaches an index as
    /// `f64` at least as often as `f32`: JSON and GraphQL have no other number
    /// type. Narrowing at the call site would make every caller decide how to
    /// do it; here it happens once, where the stored width is known.
    async fn insert<E: Element>(&mut self, id: NodeId, vector: &[E]) -> Result<()>;

    /// Removes `id`, returning whether this call was the one that did it.
    async fn delete(&mut self, id: NodeId) -> Result<bool>;

    /// Whether enough has accumulated that building would pay for itself.
    ///
    /// Defaulted false: a kind with nothing to train is never asked to build,
    /// and adding a kind does not mean remembering to answer this.
    async fn should_build(&self) -> Result<bool> {
        Ok(false)
    }

    /// Train and rebuild from what is stored.
    ///
    /// Called only when [`should_build`](Self::should_build) says so. Defaulted
    /// to a no-op for the same reason.
    async fn build(&mut self) -> Result<()> {
        Ok(())
    }

    /// Up to `k` nearest live nodes to `query`, nearest first.
    ///
    /// Takes any element width, for the same reason [`insert`](Self::insert)
    /// does. Defaulted, so a kind implements
    /// [`search_where`](Self::search_where) alone and the two cannot disagree.
    ///
    /// `effort` is how hard to look, in the kind's own unit: `ef_search` for
    /// HNSW, probes for an IVF kind, ignored by an exact one. `None` takes the
    /// kind's default. One knob rather than a per-kind options type, because a
    /// planner has to turn it without knowing which kind it holds.
    async fn search<E: Element>(
        &self,
        query: &[E],
        k: usize,
        effort: Option<usize>,
    ) -> Result<Vec<Neighbor>> {
        self.search_where(query, k, effort, &AdmitAll).await
    }

    /// Up to `k` nearest live nodes that `admit` accepts, nearest first.
    ///
    /// Filtered nearest-neighbour search, required of every kind.
    ///
    /// A selective filter never fills the `ef` result slots, so the walk keeps
    /// expanding and degrades toward a full traversal. That is bounded by the
    /// corpus, which is what the unrouted query would have read anyway.
    async fn search_where<E: Element, A: Admit>(
        &self,
        query: &[E],
        k: usize,
        effort: Option<usize>,
        admit: &A,
    ) -> Result<Vec<Neighbor>>;
}

/// Distinct from `schema::IndexKind`, which says whether an index is ordered or
/// a vector index at all. This one only distinguishes vector engines from each
/// other.
///
/// The engines that exist. The string form is what a diagnostic reports, so it
/// is defined next to them rather than spelled out at every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EngineKind {
    /// Hierarchical Navigable Small World graph.
    Hnsw,
    /// Exhaustive scan. Exact, and linear in the corpus.
    Flat,
    /// Coarse lists of product-quantized codes.
    IvfPq,
    /// Coarse lists of full-precision vectors: the same partitioning as
    /// `IvfPq`, with nothing compressed.
    IvfFlat,
    /// Satellite System Graph: one flat, angle-pruned layer.
    Ssg,
}

impl EngineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EngineKind::Hnsw => "HNSW",
            EngineKind::Flat => "FLAT",
            EngineKind::IvfPq => "IVF_PQ",
            EngineKind::IvfFlat => "IVF_FLAT",
            EngineKind::Ssg => "SSG",
        }
    }
}

/// A node and how far it is from the query.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Neighbor {
    pub id: NodeId,
    pub distance: f64,
}

/// A node paired with its distance to the current query.
///
/// It carries the node's own vector so the neighbor-selection heuristic can
/// measure candidates against each other without going back to the store. That
/// is the difference between one store read per candidate and `m` per
/// candidate.
///
/// Shared rather than owned because every hop clones a candidate into both the
/// frontier and the result set, and an embedding is a few kilobytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: NodeId,
    pub distance: f64,
    pub vector: Arc<[f32]>,
}

impl Eq for Candidate {}

impl Ord for Candidate {
    /// Nearest is *least*, so `BinaryHeap<Candidate>` pops the farthest (the
    /// result set drops its worst) and `BinaryHeap<Reverse<Candidate>>` pops
    /// the nearest (the frontier explores closest-first).
    ///
    /// `total_cmp` rather than `partial_cmp`: it is total for every `f64`
    /// including NaN, so no comparison can panic even though the metrics
    /// already promise never to produce one. The id breaks ties, which keeps
    /// heap order deterministic for equidistant nodes.
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance
            .total_cmp(&other.distance)
            .then_with(|| self.id.cmp(&other.id))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
