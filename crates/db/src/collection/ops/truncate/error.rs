use crate::error::Error;

/// Why truncating a collection was refused or failed. Each refusal Go's
/// DefraDB has carries Go's text.
#[derive(Debug, thiserror::Error)]
pub enum TruncateError {
    /// A filtered truncate on a branchable collection, whose shared history
    /// cannot be split by document.
    #[error("filtered truncate is not supported for branchable collections")]
    FilteredBranchable,
    /// A filtered truncate in an explicit transaction: truncate is not
    /// transactional.
    #[error("filtered truncate cannot run in a transaction")]
    FilteredInTransaction,
    /// The filter is empty source text.
    #[error("filter cannot be empty")]
    EmptyFilter,
    /// The filter is neither conditions nor source text that parses to them.
    #[error("unsupported filter type")]
    UnsupportedFilterType,
    /// The filter's source text did not parse, or it nests too deep.
    #[error(transparent)]
    InvalidFilter(#[from] query::QueryError),
    /// The caller may not truncate, the collection does not exist, or the
    /// store failed while truncating.
    #[error(transparent)]
    Db(#[from] Error),
}
