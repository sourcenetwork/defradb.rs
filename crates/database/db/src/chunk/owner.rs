//! Names the parent document and field a chunk step works for, in its errors and logs.

use std::fmt;

use query::error::QueryError;
use schema::CollectionVersion;

/// The parent document and `@chunk` field whose chunks a step reads or writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkOwner<'a> {
    /// The parent collection's name.
    pub collection: &'a str,
    /// The chunked field's name.
    pub field: &'a str,
    /// The parent document's DocID.
    pub doc_id: &'a str,
}

impl<'a> ChunkOwner<'a> {
    /// The parent `doc_id` of the chunks in `chunks`, named by the chunk collection's source.
    pub fn of(chunks: &'a CollectionVersion, doc_id: &'a str) -> Self {
        let (collection, field) = chunks.chunk_of.as_ref().map_or(("", ""), |source| {
            (source.collection.as_str(), source.field_name.as_str())
        });
        Self {
            collection,
            field,
            doc_id,
        }
    }

    /// `error` with its message prefixed by this owner. Only a message is
    /// changed: a conflict, a storage failure or a request for an input keeps
    /// its kind, so a retry still sees it as one.
    pub fn name_in(&self, error: QueryError) -> QueryError {
        match error {
            QueryError::Execution(message) => QueryError::Execution(format!("{self}: {message}")),
            QueryError::Internal(message) => QueryError::Internal(format!("{self}: {message}")),
            other => other,
        }
    }
}

impl fmt::Display for ChunkOwner<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "chunks of field {}.{} of document {}",
            self.collection, self.field, self.doc_id
        )
    }
}
