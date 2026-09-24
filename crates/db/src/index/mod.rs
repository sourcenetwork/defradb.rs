//! Secondary index management: maintenance, vector engines and value extraction.

pub mod backfill;
pub mod create;
pub mod error;
pub mod manager;
pub mod vector;

pub use error::{Error, Result};
pub use manager::{fulltext_index_name, BatchIndexResult, BulkIndexResult, IndexManager};
