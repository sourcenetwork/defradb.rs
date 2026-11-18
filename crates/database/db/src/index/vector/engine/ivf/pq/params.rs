//! IVF-PQ build and search parameters.

pub use crate::index::vector::engine::ivf::{MAX_NLIST, TRAIN_PER_LIST};

use crate::index::error::{Error, Result};
use crate::index::vector::engine::ivf;

/// Zero derives the probe count from `nlist`; see [`ivf::resolved_nprobe`].
pub const DEFAULT_NPROBE: u32 = 0;
pub const DEFAULT_NBITS: u32 = 8;
pub const DEFAULT_SAMPLE_BYTES: u64 = 128 << 20;
/// Candidates kept per result for the exact re-score. On 384-wide embeddings
/// 10 brings recall within 0.01 of IVF_FLAT; 1 re-scores only the top `k`.
pub const DEFAULT_RERANK: u32 = 10;
pub const MAX_RERANK: u32 = 1_000;

pub const MAX_M: u32 = 4_096;

/// `0` means derive: `nlist` from the corpus size, `m` from the width, `nprobe`
/// from `nlist`. `rerank` is how many candidates per result PQ shortlists
/// before they are re-scored against the stored vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IvfPqParams {
    pub nlist: u32,
    pub nprobe: u32,
    pub m: u32,
    pub sample_bytes: u64,
    pub rerank: u32,
}

impl Default for IvfPqParams {
    fn default() -> Self {
        Self {
            nlist: 0,
            nprobe: DEFAULT_NPROBE,
            m: 0,
            sample_bytes: DEFAULT_SAMPLE_BYTES,
            rerank: DEFAULT_RERANK,
        }
    }
}

impl IvfPqParams {
    pub fn validate(&self) -> Result<()> {
        for (name, value, max) in [
            ("nlist", self.nlist, MAX_NLIST),
            ("m", self.m, MAX_M),
            ("nprobe", self.nprobe, MAX_NLIST),
            ("rerank", self.rerank, MAX_RERANK),
        ] {
            if value > max {
                return Err(Error::Other(format!(
                    "vector index {name} is {value}, above the maximum {max}"
                )));
            }
        }
        if self.sample_bytes == 0 {
            return Err(Error::Other(
                "vector index sampleBytes must leave room for a training sample".into(),
            ));
        }
        Ok(())
    }

    /// Validate a configured subdivision once the vector width is known.
    pub fn validate_dimensions(&self, dimensions: usize) -> Result<()> {
        let m = self.m as usize;
        if m != 0 && (m > dimensions || !dimensions.is_multiple_of(m)) {
            return Err(Error::Other(format!(
                "vector index IVF-PQ m must divide the dimensions: {m} does not divide {dimensions}"
            )));
        }
        Ok(())
    }

    /// `4*sqrt(n)` is the usual starting point: lists stay large enough to
    /// train and small enough that probing a few is much cheaper than a scan.
    pub fn resolved_nlist(&self, corpus: u64) -> u32 {
        ivf::resolved_nlist(self.nlist, corpus)
    }

    /// The largest `m` that divides the width and keeps subvectors at least 2
    /// wide, capped so a code stays small against the vector it replaces.
    pub fn resolved_m(&self, dimensions: usize) -> usize {
        if self.m > 0 {
            return self.m as usize;
        }
        let target = (dimensions / 8).clamp(1, MAX_M as usize);
        (1..=target)
            .rev()
            .find(|m| dimensions.is_multiple_of(*m))
            .unwrap_or(1)
    }

    /// Lists probed when a query gives no effort of its own.
    pub fn resolved_nprobe(&self, nlist: usize) -> usize {
        ivf::resolved_nprobe(self.nprobe, nlist)
    }

    /// Vectors needed before training fires. See
    /// [`ivf::resolved_train_threshold`] for why a derived `nlist` still
    /// yields a threshold independent of the corpus.
    pub fn resolved_train_threshold(&self) -> u64 {
        ivf::resolved_train_threshold(self.nlist)
    }
}
