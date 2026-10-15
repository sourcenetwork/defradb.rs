//! Centroid training and the corpus-derived parameters both IVF engines
//! resolve the same way.

use crate::index::error::{Error, Result};
use crate::index::vector::quantize::KMeans;
use crate::index::vector::quantize::{Centroids, Clusterer};

/// FAISS's stated minimum for a usable k-means fit. Below `TRAIN_PER_LIST *
/// nlist` vectors an index stays exact rather than training on too little.
pub const TRAIN_PER_LIST: u32 = 39;

pub const MAX_NLIST: u32 = 65_536;

/// pg_vectorsearch's floor and cap on a derived probe count.
const MIN_DERIVED_NPROBE: usize = 10;
const MAX_DERIVED_NPROBE: usize = 2_048;

/// `4*sqrt(n)` is the usual starting point: lists stay large enough to train
/// and small enough that probing a few is much cheaper than a scan.
pub fn resolved_nlist(explicit: u32, corpus: u64) -> u32 {
    if explicit > 0 {
        return explicit.min(MAX_NLIST);
    }
    let derived = 4.0 * (corpus as f64).sqrt();
    (derived as u32).clamp(1, MAX_NLIST)
}

/// Lists a query probes when no effort is given. A zero `explicit` derives
/// `0.5*sqrt(nlist)`, pg_vectorsearch's default, so the share of the index a
/// query scans shrinks slowly as the corpus grows instead of falling with
/// every list added. Never more lists than exist.
pub fn resolved_nprobe(explicit: u32, nlist: usize) -> usize {
    let nprobe = if explicit > 0 {
        explicit as usize
    } else {
        ((0.5 * (nlist as f64).sqrt()).round() as usize)
            .clamp(MIN_DERIVED_NPROBE, MAX_DERIVED_NPROBE)
    };
    nprobe.min(nlist).max(1)
}

/// Vectors needed before training fires, with no dependency on the count
/// being asked about.
///
/// An explicit `nlist` already makes [`resolved_nlist`] a constant, so the
/// threshold is just `nlist * TRAIN_PER_LIST`. A derived `nlist`
/// (`explicit_nlist == 0`) makes it `4*sqrt(corpus)*TRAIN_PER_LIST`, so the
/// point it crosses solves `corpus == 4*sqrt(corpus)*TRAIN_PER_LIST`, whose
/// positive root is the fixed `16*TRAIN_PER_LIST^2`, independent of corpus.
pub fn resolved_train_threshold(explicit_nlist: u32) -> u64 {
    if explicit_nlist > 0 {
        u64::from(explicit_nlist.min(MAX_NLIST)) * u64::from(TRAIN_PER_LIST)
    } else {
        16 * u64::from(TRAIN_PER_LIST) * u64::from(TRAIN_PER_LIST)
    }
}

/// Fits `nlist` centroids over `sample`, a flat `n * dimensions` buffer,
/// failing rather than handing back a coarse quantizer with nothing in it.
pub fn fit_centroids(
    sample: &[f32],
    dimensions: usize,
    nlist: usize,
    seed: u64,
) -> Result<Centroids> {
    let clusterer = KMeans::new(seed);
    let (coarse, _) = clusterer.fit(sample, dimensions, nlist);
    if coarse.k == 0 {
        return Err(Error::Other(
            "vector index: the training sample produced no centroids".into(),
        ));
    }
    Ok(coarse)
}
