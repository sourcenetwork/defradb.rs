/// Why a document write was refused, in Go's words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The document does not exist, or the caller may not see it. Go's text.
    #[error("document not found or not authorized to access")]
    DocumentNotFound,
    /// The request was refused; its message, unchanged.
    #[error("{0}")]
    Request(String),
    /// The commit lost a write conflict. Go's text.
    #[error("transaction conflict. Please retry")]
    Conflict,
    /// The save named a document that has been deleted. Go's text.
    #[error("a document with the given ID has been deleted. DocID: {0}")]
    Deleted(String),
    /// The write named the secondary side of a relation. Go's text.
    #[error("cannot set relation from secondary side. Name: {0}")]
    SecondaryRelation(String),
}

pub type Result<T> = std::result::Result<T, Error>;
