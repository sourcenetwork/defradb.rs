//! IVF-PQ build and search parameters.

pub use crate::index::vector::engine::ivf::{MAX_NLIST, TRAIN_PER_LIST};

use crate::index::error::{Error, Result};
use crate::index::vector::engine::ivf;

pub const DEFAULT_NPROBE: u32 = 8;
pub const DEFAULT_NBITS: u32 = 8;
pub const DEFAULT_SAMPLE_BYTES: u64 = 128 << 20;

pub const MAX_M: u32 = 4_096;

/// `0` means derive from the corpus: `nlist` from its size, `m` from the width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IvfPqParams {
    pub nlist: u32,
    pub nprobe: u32,
    pub m: u32,
    pub sample_bytes: u64,
}

impl Default for IvfPqParams {
    fn default() -> Self {
        Self {
            nlist: 0,
            nprobe: DEFAULT_NPROBE,
            m: 0,
            sample_bytes: DEFAULT_SAMPLE_BYTES,
        }
    }
}

impl IvfPqParams {
    pub fn validate(&self) -> Result<()> {
        for (name, value, max) in [
            ("nlist", self.nlist, MAX_NLIST),
            ("m", self.m, MAX_M),
            ("nprobe", self.nprobe, MAX_NLIST),
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

    /// Vectors needed before training fires. See
    /// [`ivf::resolved_train_threshold`] for why a derived `nlist` still
    /// yields a threshold independent of the corpus.
    pub fn resolved_train_threshold(&self) -> u64 {
        ivf::resolved_train_threshold(self.nlist)
    }
}
