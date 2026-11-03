//! DiskANN: FreshDiskANN's FreshVamana graph, walked without anything resident
//! but the codebooks.
//!
//! An index answers by exhaustive scan, as [`Flat`] does, until it holds
//! [`TRAIN_THRESHOLD`] vectors. That write trains a product quantizer from a
//! byte-bounded sample and inserts every vector into a Vamana graph.
//!
//! Each node keeps a record of its own code and its neighbours' ids and codes,
//! so one read ranks every edge out of it. Full vectors are read only to
//! re-rank the walk's final candidates. Memory is the codebooks plus one walk,
//! and a walk expands at most a fixed multiple of its candidate list `L`, so
//! it holds `O(L * R)` ids and codes whatever the corpus size.
//!
//! Once trained, an insert stores its vector and joins a pending set with its
//! code; it writes no graph record and no shared state, so concurrent writers
//! do not conflict. A delete tombstones the node, and walks still pass through
//! any record it has until a merge consolidates it. Searches walk the graph,
//! scan the pending codes, and re-rank both.
//!
//! [`merge`](DiskAnn::merge) is the paper's StreamingMerge, in its three
//! phases: consolidate tombstones (Algorithm 4), insert pending vectors with
//! their back-edges buffered, then patch the buffered back-edges in. The
//! background sweep plans a merge on a snapshot and applies it on its own
//! transaction with point reads and writes only, so a writer appending to the
//! pending set meanwhile does not abort it.
//!
//! Unlike the paper the pending set is not an in-memory graph: it lives in the
//! store, which keeps it transactional and durable without a redo log. Every
//! pending write also appends to a commit-ordered log, so searches share an
//! in-memory copy of the set and catch it up from the log rather than scan it
//! ([`pending_cache`]).
//!
//! ADC ranks by squared distance, which orders cosine only on unit vectors and
//! Euclidean by definition, so the engine refuses only a dot-product metric.
//! The schema accepts cosine alone, as it does for IVF_PQ: Euclidean is sound
//! here but its recall has never been measured.

mod build;
mod cache;
pub mod codec;
mod graph;
mod merge;
mod params;
mod pending_cache;

#[cfg(test)]
pub(crate) use cache::BUILT as CODE_CACHES_BUILT;
pub use cache::{CodeCache, CodeCaches, Codes};
pub use codec::State;
pub use merge::MergePlan;
pub use params::{
    DiskAnnParams, CONSOLIDATE_MIN_DELETED, MAX_ALPHA_PERCENT, MAX_L, MAX_M, MAX_PLAN_PENDING,
    MAX_R, MERGE_MIN_BATCH, SLACK_PERCENT, TRAIN_THRESHOLD,
};
pub use pending_cache::{
    Copy, PendingCache, Segment, Tag, ENTRY_OVERHEAD, MAX_PENDING_CACHE_BYTES,
};

use std::sync::Arc;

use codec::{GraphState, Trained};

use crate::index::error::{Error, Result};
use crate::index::vector::engine::ann::{Admit, EngineKind, Neighbor, VectorIndexEngine};
use crate::index::vector::engine::flat::Flat;
use crate::index::vector::engine::ivf;
use crate::index::vector::engine::trained_cache::TrainedCache;
use crate::index::vector::quantize::{ProductQuantizer, Quantizer};
use crate::index::vector::store::{NodeId, VectorNodeStore};
use defra_core::vector::{Element, Metric};

#[derive(Debug, Clone)]
pub struct DiskAnn<S> {
    staging: Flat<S>,
    metric: Metric,
    params: DiskAnnParams,
    seed: u64,
    /// Fixed once trained. Shared so a write can hold it while borrowing the
    /// store mutably.
    quantizer: TrainedCache<Arc<ProductQuantizer>>,
    /// Where a resident-codes search reads codes, and where every search
    /// finds the copy of the pending list; without it each code is a point
    /// read and the pending list is scanned.
    codes: Option<Arc<CodeCache>>,
    /// Where the quantizer is shared between transactions; without it each
    /// engine reads and rebuilds it from the codebooks.
    shared: Option<Arc<CodeCache>>,
}

impl<S: VectorNodeStore> DiskAnn<S> {
    pub fn try_new(store: S, metric: Metric, params: DiskAnnParams, seed: u64) -> Result<Self> {
        params.validate()?;
        if metric != Metric::Cosine && metric != Metric::Euclidean {
            return Err(Error::Other(format!(
                "DISKANN ranks by squared distance, which does not order {metric:?}"
            )));
        }
        Ok(Self {
            staging: Flat::new(store, metric),
            metric,
            params,
            seed,
            quantizer: TrainedCache::new(),
            codes: None,
            shared: None,
        })
    }

    pub fn with_codes(mut self, codes: Arc<CodeCache>) -> Self {
        self.shared = Some(codes.clone());
        self.codes = Some(codes);
        self
    }

    /// Shares the quantizer through `cache` and nothing else: what a write,
    /// which neither walks nor scans, needs of it.
    pub fn with_shared_quantizer(mut self, cache: Arc<CodeCache>) -> Self {
        self.shared = Some(cache);
        self
    }

    pub fn store(&self) -> &S {
        self.staging.store()
    }

    pub fn store_mut(&mut self) -> &mut S {
        self.staging.store_mut()
    }

    pub fn params(&self) -> DiskAnnParams {
        self.params
    }

    /// What training fixed, or `None` while the index is still exact.
    pub(super) async fn trained(&self) -> Result<Option<Trained>> {
        match self.store().get_aux(codec::TRAINED, b"").await? {
            Some(bytes) => codec::decode_trained(&bytes).map(Some),
            None => Ok(None),
        }
    }

    pub(super) async fn graph_state(&self) -> Result<GraphState> {
        match self.store().get_aux(codec::GRAPH_STATE, b"").await? {
            Some(bytes) => codec::decode_graph_state(&bytes),
            None => Ok(GraphState::default()),
        }
    }

    pub(super) async fn put_graph_state(&mut self, state: &GraphState) -> Result<()> {
        self.store_mut()
            .put_aux(codec::GRAPH_STATE, b"", &codec::encode_graph_state(state))
            .await
    }

    /// The trained state and the graph's, or `None` while the index is exact.
    pub async fn state(&self) -> Result<Option<State>> {
        match self.trained().await? {
            Some(trained) => Ok(Some(State::new(trained, self.graph_state().await?))),
            None => Ok(None),
        }
    }

    pub(super) async fn quantizer(&self, trained: &Trained) -> Result<Arc<ProductQuantizer>> {
        if let Some(quantizer) = self.quantizer.get() {
            return Ok(quantizer.clone());
        }
        let shared = self.shared.as_ref().zip(trained.training);
        if let Some(quantizer) = shared.and_then(|(cache, training)| cache.quantizer(training)) {
            return Ok(self.quantizer.get_or_init_racy(|| quantizer).clone());
        }
        let mut books = Vec::with_capacity(trained.m as usize);
        for sub in 0..trained.m {
            let bytes = self
                .store()
                .get_aux(codec::CODEBOOK, &sub.to_be_bytes())
                .await?
                .ok_or_else(|| Error::Other(format!("vector index: codebook {sub} is missing")))?;
            books.push(ivf::decode_centroids(&bytes)?);
        }
        let quantizer = Arc::new(ProductQuantizer::from_books(
            trained.dimensions as usize,
            books,
        )?);
        if let Some((cache, training)) = shared {
            quantizer.prepare_encode();
            cache.share_quantizer(training, quantizer.clone());
        }
        Ok(self.quantizer.get_or_init_racy(|| quantizer).clone())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<S: VectorNodeStore> VectorIndexEngine for DiskAnn<S> {
    fn kind(&self) -> EngineKind {
        EngineKind::DiskAnn
    }

    /// The vector is always stored, trained or not: it is what training
    /// samples, what re-ranking reads, and what keeps the index exact until
    /// training.
    ///
    /// Once trained it joins the pending set with its code: one append to the
    /// pending log. The writes are blind, so an insert reads nothing a merge
    /// writes but the node itself.
    async fn insert<E: Element>(&mut self, id: NodeId, vector: &[E]) -> Result<()> {
        self.params.validate_dimensions(vector.len())?;
        let Some(trained) = self.trained().await? else {
            return self.staging.insert(id, vector).await;
        };
        if vector.len() != trained.dimensions as usize {
            return Err(Error::VectorDimensionMismatch {
                indexed: trained.dimensions as usize,
                got: vector.len(),
            });
        }
        self.staging.insert(id, vector).await?;
        let pq = self.quantizer(&trained).await?;
        let mut code = vec![0u8; pq.code_len()];
        pq.encode(&self.metric.prepare(vector), &mut code);
        self.store_mut()
            .delete_aux(codec::DELETED, &codec::record_key(id))
            .await?;
        self.store_mut()
            .append_pending_log(&codec::encode_log_entry(id, &code))
            .await
    }

    /// Tombstones the node: it joins the delete list, and any record it has
    /// stays for walks to pass through until a merge consolidates it.
    ///
    /// A pending insert of it stays in the log: searches and merges read the
    /// vector of every pending candidate, and drop it there.
    ///
    /// The write is blind. Merges rewrite graph records they do not own, so
    /// reading this node's record would let any overlapping merge fail the
    /// delete; a tombstone with no record is dropped by the next consolidation.
    async fn delete(&mut self, id: NodeId) -> Result<bool> {
        if !self.staging.delete(id).await? {
            return Ok(false);
        }
        if self.trained().await?.is_some() {
            self.store_mut()
                .put_aux(codec::DELETED, &codec::record_key(id), b"")
                .await?;
        }
        Ok(true)
    }

    /// Training is due at [`TRAIN_THRESHOLD`] vectors; after that a merge is
    /// due whenever anything is pending or enough tombstones accumulated.
    async fn should_build(&self) -> Result<bool> {
        match self.trained().await? {
            None => self.live_count_at_least(TRAIN_THRESHOLD).await,
            Some(_) => self.merge_due().await,
        }
    }

    /// Only training runs on a write. Deciding a merge reads the pending set,
    /// which every concurrent writer appends to; a write that read it would
    /// conflict with all of them, so merges are left to the background sweep.
    async fn should_build_inline(&self) -> Result<bool> {
        match self.trained().await? {
            None => self.live_count_at_least(TRAIN_THRESHOLD).await,
            Some(_) => Ok(false),
        }
    }

    /// Trains and builds the graph the first time; merges after that.
    async fn build(&mut self) -> Result<()> {
        match self.trained().await? {
            None => self.train_and_build().await,
            Some(_) => {
                let plan = self.merge_plan().await?;
                self.merge(plan).await
            }
        }
    }

    async fn search_where<E: Element, A: Admit>(
        &self,
        query: &[E],
        k: usize,
        effort: Option<usize>,
        admit: &A,
    ) -> Result<Vec<Neighbor>> {
        match self.trained().await? {
            None => self.staging.search_where(query, k, effort, admit).await,
            Some(trained) => self.search_graph(&trained, query, k, effort, admit).await,
        }
    }
}
