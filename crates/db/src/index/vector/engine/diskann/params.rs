//! DiskANN build and search parameters.

use crate::index::error::{Error, Result};

/// Vectors needed before the quantizer is trained. Below this the index
/// answers by exhaustive scan, which at this size is cheaper than a walk.
pub const TRAIN_THRESHOLD: u64 = 1_024;

/// Tombstones a consolidation is worth running for, whatever the live count.
pub const CONSOLIDATE_MIN_DELETED: u64 = 64;

pub const MAX_R: u32 = 1_024;
pub const MAX_L: u32 = 100_000;
pub const MAX_M: u32 = 4_096;
pub const MAX_ALPHA_PERCENT: u32 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskAnnParams {
    pub r: u32,
    pub l_build: u32,
    pub l_search: u32,
    pub alpha_percent: u32,
    pub m: u32,
    pub sample_bytes: u64,
}

impl Default for DiskAnnParams {
    fn default() -> Self {
        schema::DiskAnnParams::default().into()
    }
}

impl From<schema::DiskAnnParams> for DiskAnnParams {
    fn from(p: schema::DiskAnnParams) -> Self {
        Self {
            r: p.r,
            l_build: p.l_build,
            l_search: p.l_search,
            alpha_percent: p.alpha_percent,
            m: p.m,
            sample_bytes: p.sample_bytes,
        }
    }
}

impl DiskAnnParams {
    pub fn validate(&self) -> Result<()> {
        for (name, value, min, max) in [
            ("R", self.r, 1, MAX_R),
            ("lBuild", self.l_build, 1, MAX_L),
            ("lSearch", self.l_search, 1, MAX_L),
            ("alphaPercent", self.alpha_percent, 100, MAX_ALPHA_PERCENT),
            ("m", self.m, 0, MAX_M),
        ] {
            if !(min..=max).contains(&value) {
                return Err(Error::Other(format!(
                    "vector index DISKANN {name} is {value}, outside {min}..={max}"
                )));
            }
        }
        if self.sample_bytes == 0 {
            return Err(Error::Other(
                "vector index DISKANN sampleBytes must leave room for a training sample".into(),
            ));
        }
        Ok(())
    }

    /// Validate a configured subdivision once the vector width is known.
    pub fn validate_dimensions(&self, dimensions: usize) -> Result<()> {
        let m = self.m as usize;
        if m != 0 && (m > dimensions || !dimensions.is_multiple_of(m)) {
            return Err(Error::Other(format!(
                "vector index DISKANN m must divide the dimensions: {m} does not divide {dimensions}"
            )));
        }
        Ok(())
    }

    /// The largest `m` that divides the width and keeps subvectors at least 2
    /// wide, matching IVF-PQ's derivation.
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

    pub fn alpha(&self) -> f64 {
        f64::from(self.alpha_percent) / 100.0
    }
}
