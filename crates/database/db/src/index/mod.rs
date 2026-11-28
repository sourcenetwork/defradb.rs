//! Secondary index management: maintenance, vector engines and value extraction.

pub mod backfill;
pub mod build_gate;
pub mod create;
pub mod deferred;
pub mod drop;
mod duplicate;
pub mod error;
mod in_txn;
mod listing;
pub mod manager;
pub(crate) mod pages;
mod settle;
pub mod vector;

pub use error::{Error, Result};
pub use listing::index_listing;
pub use manager::{
    fulltext_index_description, fulltext_index_id, fulltext_index_name, BatchIndexResult,
    BulkIndexResult, IndexManager,
};
