//! Vamana's RobustPrune (DiskANN, Algorithm 3 of the FreshDiskANN paper).

use super::EdgeSelector;
use crate::index::vector::engine::ann::Candidate;
use defra_core::vector::{squared_euclidean, Metric};

/// Keeps a candidate unless an edge already kept is `alpha` times closer to it
/// than the base is.
///
/// At `alpha = 1` this is [`Heuristic`](super::Heuristic). Above 1 it keeps
/// longer edges, which is what lets a graph stay navigable across inserts and
/// deletes. Distances are Euclidean on the stored vectors whatever the index
/// metric, because that is the space `alpha` is defined in; under cosine the
/// stored vectors are unit length, so the ordering is the metric's.
#[derive(Debug, Clone, Copy)]
pub struct RobustPrune {
    alpha_squared: f64,
}

impl RobustPrune {
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha_squared: alpha * alpha,
        }
    }
}

impl EdgeSelector for RobustPrune {
    fn select(
        &self,
        _metric: Metric,
        base: &[f32],
        candidates: &[Candidate],
        max: usize,
    ) -> Vec<Candidate> {
        let mut pool: Vec<(f64, &Candidate)> = candidates
            .iter()
            .map(|candidate| (squared_euclidean(base, &candidate.vector), candidate))
            .collect();
        pool.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));

        let mut kept: Vec<Candidate> = Vec::with_capacity(max);
        for (to_base, candidate) in pool {
            if kept.len() >= max {
                break;
            }
            let covered = kept.iter().any(|edge| {
                self.alpha_squared * squared_euclidean(&edge.vector, &candidate.vector) <= to_base
            });
            if !covered {
                kept.push(candidate.clone());
            }
        }
        kept
    }
}
