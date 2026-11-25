//! Why adding collections was refused, as the HTTP API reports it.

/// A refused add-collection request: its message, and the stable code a
/// client classifies it by when the refusal has one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct CollectionRefusal {
    pub message: String,
    pub code: Option<&'static str>,
}

impl From<String> for CollectionRefusal {
    fn from(message: String) -> Self {
        Self {
            message,
            code: None,
        }
    }
}

impl From<db::AddCollectionError> for CollectionRefusal {
    fn from(error: db::AddCollectionError) -> Self {
        Self {
            code: error.code(),
            message: error.to_string(),
        }
    }
}
