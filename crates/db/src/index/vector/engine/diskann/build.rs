//! Training and the first build.

use std::sync::Arc;

use super::codec::{self, GraphState, Trained};
use super::DiskAnn;
use crate::index::error::{Error, Result};
use crate::index::vector::engine::ivf;
use crate::index::vector::quantize::{KMeans, ProductQuantizer, Reservoir, Sampler};
use crate::index::vector::store::{NodeId, VectorNodeStore};
use defra_core::vector::squared_euclidean;

impl<S: VectorNodeStore> DiskAnn<S> {
    /// Whether at least `wanted` live vectors are stored, counting no further.
    /// See `IvfPq::live_count_at_least` for the early-stop signal.
    pub(super) async fn live_count_at_least(&self, wanted: u64) -> Result<bool> {
        let mut count = 0u64;
        match self
            .store()
            .iterate_nodes(|_| {
                count += 1;
                if count < wanted {
                    Ok(())
                } else {
                    Err(Error::Other(
                        "vector index: live_count_at_least stopped early".into(),
                    ))
                }
            })
            .await
        {
            Ok(()) => Ok(false),
            Err(_) if count >= wanted => Ok(true),
            Err(err) => Err(err),
        }
    }

    /// Trains the quantizer from a byte-bounded sample, then inserts every
    /// stored vector into the graph, the medoid first so it is the entry.
    pub(super) async fn train_and_build(&mut self) -> Result<()> {
        let mut dimensions = 0;
        let mut reservoir = None;
        // The insert pass writes the store, which the scan holds borrowed, so
        // the ids are collected first.
        let mut ids = Vec::new();
        let sample_bytes = self.params.sample_bytes as usize;
        let seed = self.seed;
        self.store()
            .iterate_nodes(|node| {
                let sample = reservoir.get_or_insert_with(|| {
                    dimensions = node.vector.len();
                    Reservoir::new(dimensions, sample_bytes, seed)
                });
                if node.vector.len() != dimensions {
                    return Err(Error::VectorDimensionMismatch {
                        indexed: dimensions,
                        got: node.vector.len(),
                    });
                }
                sample.offer(&node.vector);
                ids.push(node.id);
                Ok(())
            })
            .await?;
        let reservoir = reservoir.ok_or_else(|| {
            Error::Other("vector index: nothing to train a DISKANN build on".into())
        })?;
        self.params.validate_dimensions(dimensions)?;

        let m = self.params.resolved_m(dimensions);
        let quantizer =
            ProductQuantizer::train(&KMeans::new(seed), reservoir.as_flat(), dimensions, m)?;
        for (sub, book) in quantizer.books().iter().enumerate() {
            self.store_mut()
                .put_aux(
                    codec::CODEBOOK,
                    &(sub as u32).to_be_bytes(),
                    &ivf::encode_centroids(book),
                )
                .await?;
        }
        quantizer.prepare_encode();
        let _ = self.quantizer.set(Arc::new(quantizer));

        let medoid = self
            .nearest_to_mean(reservoir.as_flat(), dimensions)
            .await?;
        let trained = Trained {
            dimensions: dimensions as u32,
            m: m as u32,
            training: Some(rand::random()),
        };
        let mut graph = GraphState::default();
        for id in std::iter::once(medoid).chain(ids.into_iter().filter(|id| *id != medoid)) {
            let Some(node) = self.store().get_node(id).await? else {
                continue;
            };
            self.graph_insert(&trained, &mut graph, id, &node.vector)
                .await?;
        }
        self.put_graph_state(&graph).await?;
        // Written last: an insert reads this to decide the index is trained.
        self.store_mut()
            .put_aux(codec::TRAINED, b"", &codec::encode_trained(&trained))
            .await
    }

    /// The stored node nearest the sample's mean: the walk's start, so every
    /// walk begins near the middle of the data.
    async fn nearest_to_mean(&self, sample: &[f32], dimensions: usize) -> Result<NodeId> {
        let mut mean = vec![0.0f32; dimensions];
        let rows = sample.chunks_exact(dimensions);
        let count = rows.len().max(1) as f32;
        for row in rows {
            for (slot, value) in mean.iter_mut().zip(row) {
                *slot += value / count;
            }
        }
        let mut best: Option<(f64, NodeId)> = None;
        self.store()
            .iterate_nodes(|node| {
                let distance = squared_euclidean(&mean, &node.vector);
                if best.is_none_or(|(nearest, _)| distance < nearest) {
                    best = Some((distance, node.id));
                }
                Ok(())
            })
            .await?;
        best.map(|(_, id)| id)
            .ok_or_else(|| Error::Other("vector index: nothing to build a DISKANN graph on".into()))
    }
}
