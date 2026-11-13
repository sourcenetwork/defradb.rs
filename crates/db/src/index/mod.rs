//! Secondary index management: maintenance, vector engines and value extraction.

pub mod backfill;
pub mod build_gate;
pub mod create;
pub mod drop;
pub mod error;
mod in_txn;
mod listing;
pub mod manager;
pub(crate) mod pages;
pub mod vector;

pub use error::{Error, Result};
pub use listing::index_listing;
pub use manager::{fulltext_index_name, BatchIndexResult, BulkIndexResult, IndexManager};
