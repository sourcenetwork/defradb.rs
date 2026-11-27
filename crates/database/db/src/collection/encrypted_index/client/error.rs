/// Why an encrypted index operation was refused. HTTP, the CLI, FFI and the
/// embedded node all report these texts unchanged.
#[derive(Debug, thiserror::Error)]
pub enum EncryptedIndexError {
    /// The node refused the caller, the definition change was refused, or
    /// storage failed.
    #[error(transparent)]
    Db(crate::Error),
    /// The named collection does not exist. Go's text.
    #[error("collection not found. Name: {0}")]
    CollectionMissing(String),
    /// The transaction the request names is not open. Go's text.
    #[error("transaction not found")]
    TransactionNotFound,
}

impl From<crate::Error> for EncryptedIndexError {
    fn from(error: crate::Error) -> Self {
        match error {
            crate::Error::CollectionNotFound(name) => Self::CollectionMissing(name),
            crate::Error::TransactionNotFound(_) => Self::TransactionNotFound,
            error => Self::Db(error),
        }
    }
}
