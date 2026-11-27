use crate::executor::{DOCUMENT_ALREADY_EXISTS_ERROR_CODE, DOCUMENT_DELETED_ERROR_CODE};

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
    /// The create's document ID is already stored. Go's text.
    #[error("a document with the given ID already exists. DocID: {0}")]
    AlreadyExists(String),
    /// The save named a document that has been deleted. Go's text.
    #[error("a document with the given ID has been deleted. DocID: {0}")]
    Deleted(String),
    /// The write named a collection that does not exist. Go's text.
    #[error("collection not found")]
    CollectionNotFound,
    /// The write named the secondary side of a relation. Go's text.
    #[error("cannot set relation from secondary side. Name: {0}")]
    SecondaryRelation(String),
}

impl Error {
    /// The stable code a client reports with this refusal, beside Go's text:
    /// [`DOCUMENT_ALREADY_EXISTS_ERROR_CODE`] or [`DOCUMENT_DELETED_ERROR_CODE`].
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::AlreadyExists(_) => Some(DOCUMENT_ALREADY_EXISTS_ERROR_CODE),
            Self::Deleted(_) => Some(DOCUMENT_DELETED_ERROR_CODE),
            Self::DocumentNotFound
            | Self::Request(_)
            | Self::Conflict
            | Self::CollectionNotFound
            | Self::SecondaryRelation(_) => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
