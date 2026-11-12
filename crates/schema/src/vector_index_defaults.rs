//! Go's defaults for a vector index description that leaves parts unset.
//!
//! Go's index API writes an unset algorithm or metric as `""` and an HNSW
//! block with zero parameters, and fills in the defaults when it creates the
//! index (internal/db/collection_index.go validateVectorIndexDescription).
//!
//! The stored form is the normalized one; `proofs/lean/VectorIndexDefaults.lean`
//! proves normalizing is idempotent and leaves no zero HNSW parameter.

use serde::de::{Error as _, IntoDeserializer};
use serde::{Deserialize, Deserializer};

use crate::{HnswParams, VectorAlgorithm, VectorIndexDescription};

impl VectorIndexDescription {
    /// This description with Go's defaults filled in for an HNSW index: no
    /// HNSW block means an empty one, and a zero parameter means its default.
    pub fn normalized(mut self) -> Self {
        if self.algorithm == VectorAlgorithm::Hnsw {
            let defaults = HnswParams::default();
            let mut params = self.hnsw.unwrap_or(defaults);
            if params.m == 0 {
                params.m = defaults.m;
            }
            if params.ef_construction == 0 {
                params.ef_construction = defaults.ef_construction;
            }
            if params.ef_search == 0 {
                params.ef_search = defaults.ef_search;
            }
            self.hnsw = Some(params);
        }
        self
    }
}

/// Reads an enum that Go writes as an empty string when unset, giving the
/// enum's default for the empty string.
pub(crate) fn empty_is_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    let name = String::deserialize(deserializer)?;
    if name.is_empty() {
        return Ok(T::default());
    }
    T::deserialize(name.into_deserializer())
        .map_err(|error: serde::de::value::Error| D::Error::custom(error))
}
