//! Probing lists and scanning codes.

use std::collections::BinaryHeap;

use super::codec::{self, TrainedState};
use super::IvfPq;
use crate::index::error::Result;
use crate::index::vector::engine::ann::{Admit, Neighbor};
use crate::index::vector::engine::ivf;
use crate::index::vector::quantize::Quantizer;
use crate::index::vector::store::{NodeId, VectorNodeStore};
use defra_core::vector::Element;

impl<S: VectorNodeStore> IvfPq<S> {
    pub(super) async fn search_lists<E: Element, A: Admit>(
        &self,
        state: &TrainedState,
        query: &[E],
        k: usize,
        effort: Option<usize>,
        admit: &A,
    ) -> Result<Vec<Neighbor>> {
        if k == 0 {
            return Ok(Vec::new());
        }

        let query = self.metric().prepare(query);

        let (coarse, quantizer) = self.trained_parts(state).await?;

        // `effort` overrides nprobe the way ef_search does for a graph.
        let lists = ivf::probe_lists(query.as_slice(), coarse, self.params().nprobe, effort);

        // PQ only shortlists. Keeping `rerank` candidates per result and
        // re-scoring them against the stored vectors is what lets the answer
        // reach IVF_FLAT's recall rather than stall at the codec's.
        let pool = k.saturating_mul(self.params().rerank.max(1) as usize);
        let mut best: BinaryHeap<Ranked> = BinaryHeap::with_capacity(pool + 1);
        let mut residual = vec![0.0f32; state.dimensions as usize];

        for list in lists {
            for (slot, (q, c)) in residual.iter_mut().zip(query.iter().zip(coarse.get(list))) {
                *slot = q - c;
            }
            let table = quantizer.distance_table(&residual);

            let mut hits: Vec<(NodeId, f64)> = Vec::new();
            self.store()
                .iterate_aux(
                    codec::LIST,
                    &codec::list_prefix(list as u32),
                    |key, code| {
                        let id = codec::node_from_list_key(key)?;
                        if admit.admits(id) {
                            hits.push((id, quantizer.distance(&table, code)));
                        }
                        Ok(())
                    },
                )
                .await?;

            for (id, distance) in hits {
                best.push(Ranked { id, distance });
                if best.len() > pool {
                    best.pop();
                }
            }
        }

        // Read each survivor's full vector for reranking. A list written
        // before delete removed codes may still name a tombstone.
        let metric = self.metric();
        let mut out = Vec::with_capacity(best.len());
        for candidate in best.into_vec() {
            let Some(node) = self.store().get_node(candidate.id).await? else {
                continue;
            };
            if node.deleted {
                continue;
            }
            out.push(Neighbor {
                id: candidate.id,
                distance: metric.distance_stored(query.as_slice(), &node.vector),
            });
        }
        out.sort_by(|a, b| {
            a.distance
                .total_cmp(&b.distance)
                .then_with(|| a.id.cmp(&b.id))
        });
        out.truncate(k);
        Ok(out)
    }
}

/// Farthest on top, so the heap drops its worst.
#[derive(Debug, PartialEq)]
struct Ranked {
    id: NodeId,
    distance: f64,
}

impl Eq for Ranked {}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.distance
            .total_cmp(&other.distance)
            .then_with(|| self.id.cmp(&other.id))
    }
}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
