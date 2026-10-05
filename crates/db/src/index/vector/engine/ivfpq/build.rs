//! Training and the build that follows it.

use super::codec::{self, TrainedState};
use super::IvfPq;
use crate::index::error::{Error, Result};
use crate::index::vector::engine::ann::{Centroids, Quantizer, Sampler};
use crate::index::vector::engine::ivf;
use crate::index::vector::quantize::{KMeans, ProductQuantizer, Reservoir};
use crate::index::vector::store::VectorNodeStore;

/// What a build did, so a caller can report it rather than guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildReport {
    pub sampled: usize,
    pub sample_bytes: usize,
    pub indexed: u64,
    pub state: TrainedState,
}

impl<S: VectorNodeStore> IvfPq<S> {
    /// Vectors held, tombstones excluded.
    pub async fn live_count(&self) -> Result<u64> {
        let mut count = 0u64;
        self.store()
            .iterate_nodes(|_| {
                count += 1;
                Ok(())
            })
            .await?;
        Ok(count)
    }

    /// Whether at least `wanted` live vectors are stored, counting no further.
    ///
    /// The count is bounded by what the question needs rather than by the
    /// corpus, so asking it on every write costs a constant rather than a scan.
    /// `iterate_nodes` stops as soon as the visitor returns an error, so the
    /// visitor becomes that stop signal itself once `wanted` is reached; `count`
    /// having reached `wanted` is what distinguishes the signal from a real
    /// failure the store raised on its own.
    pub async fn live_count_at_least(&self, wanted: u64) -> Result<bool> {
        if wanted == 0 {
            return Ok(true);
        }
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

    /// Whether there are enough vectors to fit the configured lists.
    pub async fn should_build(&self) -> Result<bool> {
        if self.is_trained().await? {
            return Ok(false);
        }
        self.live_count_at_least(self.params().resolved_train_threshold())
            .await
    }

    /// Trains from a byte-bounded sample and writes centroids, codebooks and
    /// inverted lists.
    ///
    /// Two streaming passes over the nodes and nothing else resident: the
    /// sample, the centroids and the codebooks, each bounded by configuration
    /// rather than by the corpus.
    pub async fn build(&mut self) -> Result<BuildReport> {
        let mut live = 0;
        let mut dimensions = 0;
        let mut reservoir = None;
        self.store()
            .iterate_nodes(|node| {
                if node.vector.is_empty() {
                    return Err(Error::Other(
                        "vector index: stored vector has no dimensions".into(),
                    ));
                }
                let sample = reservoir.get_or_insert_with(|| {
                    dimensions = node.vector.len();
                    Reservoir::new(dimensions, self.params().sample_bytes as usize, self.seed())
                });
                if node.vector.len() != dimensions {
                    return Err(Error::VectorDimensionMismatch {
                        indexed: dimensions,
                        got: node.vector.len(),
                    });
                }
                live += 1;
                sample.offer(&node.vector);
                Ok(())
            })
            .await?;
        let reservoir = reservoir.ok_or_else(|| {
            Error::Other("vector index: nothing to train an IVF-PQ build on".into())
        })?;
        let nlist = self.params().resolved_nlist(live);
        let m = self.params().resolved_m(dimensions);

        let sampled = reservoir.len();
        let sample_bytes = reservoir.resident_bytes();
        let coarse =
            ivf::fit_centroids(reservoir.as_flat(), dimensions, nlist as usize, self.seed())?;

        let residuals = residuals_of(reservoir.as_flat(), dimensions, &coarse);
        let clusterer = KMeans::new(self.seed());
        let quantizer = ProductQuantizer::train(&clusterer, &residuals, dimensions, m)?;
        drop(residuals);

        for index in 0..coarse.k {
            let bytes = codec::encode_vector(coarse.get(index));
            self.store_mut()
                .put_aux(codec::CENTROID, &(index as u32).to_be_bytes(), &bytes)
                .await?;
        }
        for (sub, book) in quantizer.books().iter().enumerate() {
            let bytes = codec::encode_centroids(book);
            self.store_mut()
                .put_aux(codec::CODEBOOK, &(sub as u32).to_be_bytes(), &bytes)
                .await?;
        }

        let state = TrainedState {
            nlist: coarse.k as u32,
            m: quantizer.m() as u32,
            dimensions: dimensions as u32,
        };

        let mut code = vec![0u8; quantizer.code_len()];
        let mut residual = vec![0.0f32; dimensions];
        let indexed = self
            .store_mut()
            .write_aux_from_nodes(codec::LIST, |node| {
                let (list, _) = coarse.nearest(&node.vector);
                subtract_into(&node.vector, coarse.get(list), &mut residual);
                quantizer.encode(&residual, &mut code);
                Ok((codec::list_key(list as u32, node.id), code.clone()))
            })
            .await?;

        self.store_mut()
            .put_aux(codec::STATE, b"", &codec::encode_state(&state))
            .await?;

        Ok(BuildReport {
            sampled,
            sample_bytes,
            indexed,
            state,
        })
    }

    /// The list a vector belongs to, and its code.
    pub(super) async fn assign(
        &self,
        state: &TrainedState,
        vector: &[f32],
    ) -> Result<(u32, Vec<u8>)> {
        let (coarse, quantizer) = self.trained_parts(state).await?;
        let (list, _) = coarse.nearest(vector);
        let mut residual = vec![0.0f32; state.dimensions as usize];
        subtract_into(vector, coarse.get(list), &mut residual);
        let mut code = vec![0u8; quantizer.code_len()];
        quantizer.encode(&residual, &mut code);
        Ok((list as u32, code))
    }

    pub(super) async fn load_coarse_centroids(&self, state: &TrainedState) -> Result<Centroids> {
        ivf::load_centroids(self.store(), state.nlist, state.dimensions).await
    }
}

fn subtract_into(vector: &[f32], centroid: &[f32], out: &mut [f32]) {
    for (slot, (v, c)) in out.iter_mut().zip(vector.iter().zip(centroid)) {
        *slot = v - c;
    }
}

fn residuals_of(sample: &[f32], dimensions: usize, coarse: &Centroids) -> Vec<f32> {
    let mut residuals = vec![0.0f32; sample.len()];
    for (point, slot) in sample
        .chunks_exact(dimensions)
        .zip(residuals.chunks_exact_mut(dimensions))
    {
        let (nearest, _) = coarse.nearest(point);
        subtract_into(point, coarse.get(nearest), slot);
    }
    residuals
}
