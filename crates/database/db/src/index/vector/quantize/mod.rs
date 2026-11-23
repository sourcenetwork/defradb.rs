//! Sampling, clustering and quantization, each trait beside its
//! implementation. Nothing here imports storage.

mod kmeans;
mod pq;
mod sample;

pub use kmeans::{Centroids, Clusterer, Fit, KMeans};
pub use pq::{ProductQuantizer, Quantizer, CODEBOOK_SIZE};
pub use sample::{Reservoir, Sampler};
