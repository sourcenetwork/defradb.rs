//! Training, the first build, and consolidating tombstones.

use std::collections::HashSet;
use std::sync::Arc;

use super::codec::{self, Record, State};
use super::graph::decode;
use super::DiskAnn;
use crate::index::error::{Error, Result};
use crate::index::vector::engine::ivf;
use crate::index::vector::quantize::{KMeans, ProductQuantizer, Quantizer, Reservoir, Sampler};
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
        let _ = self.quantizer.set(Arc::new(quantizer));

        let medoid = self
            .nearest_to_mean(reservoir.as_flat(), dimensions)
            .await?;
        let mut state = State {
            dimensions: dimensions as u32,
            m: m as u32,
            entry: None,
            live: 0,
            deleted: 0,
        };
        for id in std::iter::once(medoid).chain(ids.into_iter().filter(|id| *id != medoid)) {
            let Some(node) = self.store().get_node(id).await? else {
                continue;
            };
            self.graph_insert(&mut state, id, &node.vector).await?;
        }
        self.put_state(&state).await
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

    /// Delete consolidation (paper Algorithm 4): every node with an edge to a
    /// tombstone takes that tombstone's out-edges in its place, re-pruned, and
    /// the tombstones' records are dropped.
    ///
    /// The delete list and the ids of the nodes it touches are resident, as in
    /// the paper: bounded by the deletes being consolidated, not the corpus.
    pub(super) async fn consolidate(&mut self, mut state: State) -> Result<()> {
        let pq = self.quantizer(&state).await?;
        let m = pq.code_len();

        let mut deleted = HashSet::new();
        self.store()
            .iterate_aux(codec::DELETED, b"", |key, _| {
                deleted.insert(codec::id_from_key(key)?);
                Ok(())
            })
            .await?;

        let mut affected = Vec::new();
        self.store()
            .iterate_aux(codec::GRAPH, b"", |key, value| {
                let id = codec::id_from_key(key)?;
                if deleted.contains(&id) {
                    return Ok(());
                }
                let record = codec::decode_record(value, m)?;
                if record.neighbors.iter().any(|(n, _)| deleted.contains(n)) {
                    affected.push(id);
                }
                Ok(())
            })
            .await?;

        let mut fallback_entry = None;
        for id in affected {
            let Some(record) = self.record(m, id).await? else {
                continue;
            };
            let mut edges = Vec::with_capacity(record.neighbors.len());
            for (neighbor, code) in record.neighbors {
                if !deleted.contains(&neighbor) {
                    edges.push((neighbor, code));
                    continue;
                }
                if let Some(gone) = self.record(m, neighbor).await? {
                    edges.extend(
                        gone.neighbors
                            .into_iter()
                            .filter(|(n, _)| *n != id && !deleted.contains(n)),
                    );
                }
            }
            let base = decode(&pq, &record.code);
            let neighbors = self.prune(&pq, &base, edges);
            self.put_record(
                id,
                &Record {
                    code: record.code,
                    neighbors,
                },
            )
            .await?;
            fallback_entry.get_or_insert(id);
        }

        if state.entry.is_some_and(|entry| deleted.contains(&entry)) {
            state.entry = match fallback_entry {
                Some(id) => Some(id),
                None => self.any_live_record(&deleted).await?,
            };
        }
        for id in &deleted {
            let key = codec::record_key(*id);
            self.store_mut().delete_aux(codec::GRAPH, &key).await?;
            self.store_mut().delete_aux(codec::DELETED, &key).await?;
        }
        state.live = state.live.saturating_sub(deleted.len() as u64);
        state.deleted = 0;
        self.put_state(&state).await
    }

    async fn any_live_record(&self, deleted: &HashSet<NodeId>) -> Result<Option<NodeId>> {
        let mut found = None;
        let stop = "vector index: any_live_record stopped early";
        match self
            .store()
            .iterate_aux(codec::GRAPH, b"", |key, _| {
                let id = codec::id_from_key(key)?;
                if deleted.contains(&id) {
                    return Ok(());
                }
                found = Some(id);
                Err(Error::Other(stop.into()))
            })
            .await
        {
            Ok(()) => Ok(None),
            Err(_) if found.is_some() => Ok(found),
            Err(err) => Err(err),
        }
    }
}
