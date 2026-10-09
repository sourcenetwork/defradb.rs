//! DiskANN: FreshDiskANN's FreshVamana graph, walked without anything resident
//! but the codebooks.
//!
//! An index answers by exhaustive scan, as [`Flat`] does, until it holds
//! [`TRAIN_THRESHOLD`] vectors. It then trains a product quantizer from a
//! byte-bounded sample and inserts every vector into a Vamana graph.
//!
//! Each node keeps a record of its own code and its neighbours' ids and codes,
//! so one read ranks every edge out of it. Full vectors are read only to
//! re-rank the walk's final candidates. Memory is the codebooks plus one walk's
//! candidate list, whatever the corpus size.
//!
//! Inserts and deletes follow FreshVamana: edges are pruned with `alpha > 1`,
//! and a delete tombstones the node, which a walk still passes through. Once
//! tombstones reach a tenth of the graph, [`build`](VectorIndexEngine::build)
//! consolidates them (the paper's Algorithm 4).
//!
//! Unlike the paper there is no in-memory temporary index and no streaming
//! merge: an insert goes straight into the graph on the writing transaction,
//! which keeps the index transactional without any state between calls.
//!
//! ADC ranks by squared distance, which orders cosine only on unit vectors and
//! Euclidean by definition, so a dot-product metric is refused.

mod build;
pub mod codec;
mod graph;
mod params;

pub use codec::State;
pub use params::{
    DiskAnnParams, CONSOLIDATE_MIN_DELETED, MAX_ALPHA_PERCENT, MAX_L, MAX_M, MAX_R, TRAIN_THRESHOLD,
};

use std::sync::{Arc, OnceLock};

use crate::index::error::{Error, Result};
use crate::index::vector::engine::ann::{Admit, EngineKind, Neighbor, VectorIndexEngine};
use crate::index::vector::engine::flat::Flat;
use crate::index::vector::engine::ivf;
use crate::index::vector::quantize::ProductQuantizer;
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
    quantizer: OnceLock<Arc<ProductQuantizer>>,
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
            quantizer: OnceLock::new(),
        })
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

    /// The trained state, or `None` while the index is still exact.
    pub async fn state(&self) -> Result<Option<State>> {
        match self.store().get_aux(codec::STATE, b"").await? {
            Some(bytes) => codec::decode_state(&bytes).map(Some),
            None => Ok(None),
        }
    }

    async fn put_state(&mut self, state: &State) -> Result<()> {
        self.store_mut()
            .put_aux(codec::STATE, b"", &codec::encode_state(state))
            .await
    }

    async fn quantizer(&self, state: &State) -> Result<Arc<ProductQuantizer>> {
        if let Some(quantizer) = self.quantizer.get() {
            return Ok(quantizer.clone());
        }
        let mut books = Vec::with_capacity(state.m as usize);
        for sub in 0..state.m {
            let bytes = self
                .store()
                .get_aux(codec::CODEBOOK, &sub.to_be_bytes())
                .await?
                .ok_or_else(|| Error::Other(format!("vector index: codebook {sub} is missing")))?;
            books.push(ivf::decode_centroids(&bytes)?);
        }
        let quantizer = Arc::new(ProductQuantizer::from_books(
            state.dimensions as usize,
            books,
        )?);
        Ok(self.quantizer.get_or_init(|| quantizer).clone())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<S: VectorNodeStore> VectorIndexEngine for DiskAnn<S> {
    fn kind(&self) -> EngineKind {
        EngineKind::DiskAnn
    }

    /// The vector is always stored, trained or not: it is what training samples,
    /// what re-ranking reads, and what keeps the index exact until training.
    async fn insert<E: Element>(&mut self, id: NodeId, vector: &[E]) -> Result<()> {
        self.params.validate_dimensions(vector.len())?;
        let Some(mut state) = self.state().await? else {
            return self.staging.insert(id, vector).await;
        };
        if vector.len() != state.dimensions as usize {
            return Err(Error::VectorDimensionMismatch {
                indexed: state.dimensions as usize,
                got: vector.len(),
            });
        }
        self.staging.insert(id, vector).await?;
        let key = codec::record_key(id);
        if self.store().get_aux(codec::DELETED, &key).await?.is_some() {
            self.store_mut().delete_aux(codec::DELETED, &key).await?;
            state.deleted = state.deleted.saturating_sub(1);
        }
        let prepared = self.metric.prepare(vector);
        self.graph_insert(&mut state, id, &prepared).await?;
        self.put_state(&state).await
    }

    /// Tombstones the node. Once trained it also joins the delete list, and
    /// its record stays in the graph for walks to pass through until
    /// consolidation.
    async fn delete(&mut self, id: NodeId) -> Result<bool> {
        if !self.staging.delete(id).await? {
            return Ok(false);
        }
        if let Some(mut state) = self.state().await? {
            let key = codec::record_key(id);
            if self.store().get_aux(codec::GRAPH, &key).await?.is_some() {
                self.store_mut().put_aux(codec::DELETED, &key, b"").await?;
                state.deleted += 1;
                self.put_state(&state).await?;
            }
        }
        Ok(true)
    }

    async fn should_build(&self) -> Result<bool> {
        match self.state().await? {
            None => self.live_count_at_least(TRAIN_THRESHOLD).await,
            Some(state) => Ok(state.deleted >= CONSOLIDATE_MIN_DELETED.max(state.live / 10)),
        }
    }

    /// Trains and builds the graph the first time; consolidates tombstones
    /// after that.
    async fn build(&mut self) -> Result<()> {
        match self.state().await? {
            None => self.train_and_build().await,
            Some(state) => self.consolidate(state).await,
        }
    }

    async fn search_where<E: Element, A: Admit>(
        &self,
        query: &[E],
        k: usize,
        effort: Option<usize>,
        admit: &A,
    ) -> Result<Vec<Neighbor>> {
        match self.state().await? {
            None => self.staging.search_where(query, k, effort, admit).await,
            Some(state) => self.search_graph(&state, query, k, effort, admit).await,
        }
    }
}
