//! Vector indexing.
//!
//! Layered so a future index kind supplies only a new engine: `store` holds the
//! persistence port and its adapters, `engine` the index kinds, `quantize` the
//! sampling, clustering and quantization they train with. Metric and distance
//! primitives live in `defra_core::vector`. Nothing below `store` knows what a
//! database is.
pub mod codec;
pub mod engine;
pub mod index;
pub mod params;
pub mod quantize;
pub mod rebuild_task;
pub mod store;
