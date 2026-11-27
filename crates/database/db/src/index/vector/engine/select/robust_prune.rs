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

    /// RobustPrune over candidates whose `distance` is already their squared
    /// euclidean distance to the base, as [`select`](EdgeSelector::select)
    /// computes it: a caller that has that distance in hand skips computing it
    /// twice, and keeps the same edges.
    pub fn select_ranked(&self, candidates: &[Candidate], max: usize) -> Vec<Candidate> {
        self.keep(
            candidates
                .iter()
                .map(|candidate| (candidate.distance, candidate))
                .collect(),
            max,
        )
    }

    /// `pool` is each candidate with its squared distance to the base.
    fn keep(&self, mut pool: Vec<(f64, &Candidate)>, max: usize) -> Vec<Candidate> {
        pool.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));

        // A candidate is covered by an edge kept before its block or by one kept
        // earlier in its block: the same `any` as checking each in turn, so the
        // same edges are kept. A candidate an edge has covered is not measured
        // against the edges after it: its answer can no longer change.
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
            let mut covered = [false; BLOCK];
            let mut open: [usize; BLOCK] = std::array::from_fn(|k| k);
            let mut left = block.len();
            for edge in &kept {
                let distances = distances(&edge.vector, &wide, &open[..left]);
                let mut still = 0;
                for (slot, distance) in distances.into_iter().take(left).enumerate() {
                    let k = open[slot];
                    if self.alpha_squared * distance <= block[k].0 {
                        covered[k] = true;
                    } else {
                        open[still] = k;
                        still += 1;
                    }
                }
                left = still;
                if left == 0 {
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

/// `edge`'s squared distance to each candidate of the block still open, in
/// `open`'s order: one block call sized to what is left, so a covered
/// candidate costs nothing more.
fn distances(edge: &[f32], wide: &[Vec<f64>], open: &[usize]) -> [f64; BLOCK] {
    match open.len() {
        1 => sized::<1>(edge, wide, open),
        2 => sized::<2>(edge, wide, open),
        3 => sized::<3>(edge, wide, open),
        4 => sized::<4>(edge, wide, open),
        5 => sized::<5>(edge, wide, open),
        _ => sized::<BLOCK>(edge, wide, open),
    }
}

fn sized<const B: usize>(edge: &[f32], wide: &[Vec<f64>], open: &[usize]) -> [f64; BLOCK] {
    let slots: [&[f64]; B] = std::array::from_fn(|k| wide[open[k]].as_slice());
    let mut out = [0.0; BLOCK];
    out[..B].copy_from_slice(&squared_euclidean_block(edge, slots));
    out
}

impl EdgeSelector for RobustPrune {
    fn select(
        &self,
        _metric: Metric,
        base: &[f32],
        candidates: &[Candidate],
        max: usize,
    ) -> Vec<Candidate> {
        self.keep(
            candidates
                .iter()
                .map(|candidate| (squared_euclidean(base, &candidate.vector), candidate))
                .collect(),
            max,
        )
    }
}
