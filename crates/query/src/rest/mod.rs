//! REST reads: collection listing and documents by ID.

mod error;
mod gql;
mod operations;
mod trait_def;

pub use error::{RestError, RestResult};
pub use operations::RestOperationsImpl;
pub use trait_def::{CollectionDocIdsPage, CollectionDocIdsPagination, RestOperations};

#[cfg(test)]
mod tests;
