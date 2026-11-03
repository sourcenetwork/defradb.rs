//! Vamana's RobustPrune (DiskANN, Algorithm 3 of the FreshDiskANN paper).

use super::EdgeSelector;
use crate::index::vector::engine::ann::Candidate;
use defra_core::vector::{squared_euclidean, squared_euclidean_block, Metric};

/// Candidates checked together against the edges kept before them. The widened
/// block stays in L1 while each edge is read once for all of it; on the bench
/// machine 6 beat 4 and 8.
const BLOCK: usize = 6;

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

        // A candidate is covered by an edge kept before its block or by one kept
        // earlier in its block: the same `any` as checking each in turn, so the
        // same edges are kept.
        let mut kept: Vec<Candidate> = Vec::with_capacity(max);
        let mut wide: Vec<Vec<f64>> = vec![Vec::new(); BLOCK];
        for block in pool.chunks(BLOCK) {
            if kept.len() >= max {
                break;
            }
            for (wide, (_, candidate)) in wide.iter_mut().zip(block) {
                wide.clear();
                wide.extend(candidate.vector.iter().map(|&x| f64::from(x)));
            }
            let slots: [&[f64]; BLOCK] =
                std::array::from_fn(|k| wide[k.min(block.len() - 1)].as_slice());
            let mut covered = [false; BLOCK];
            for edge in &kept {
                let distances = squared_euclidean_block(&edge.vector, slots);
                for (covered, (distance, (to_base, _))) in
                    covered.iter_mut().zip(distances.iter().zip(block))
                {
                    *covered |= self.alpha_squared * distance <= *to_base;
                }
                if covered[..block.len()].iter().all(|&c| c) {
                    break;
                }
            }
            let before = kept.len();
            for (&(to_base, candidate), covered) in block.iter().zip(covered) {
                if kept.len() >= max {
                    break;
                }
                let covered = covered
                    || kept[before..].iter().any(|edge| {
                        self.alpha_squared * squared_euclidean(&edge.vector, &candidate.vector)
                            <= to_base
                    });
                if !covered {
                    kept.push(candidate.clone());
                }
            }
        }
        kept
    }
}
