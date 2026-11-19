//! Refuses user writes to a chunk collection.

use schema::CollectionVersion;

use crate::error::{QueryError, Result};

/// Refuses a user write to a chunk collection. Only the parent's writes change chunks.
pub fn ensure_user_writable(collection: &CollectionVersion) -> Result<()> {
    match &collection.chunk_of {
        Some(parent) => Err(QueryError::execution(format!(
            "{} holds the chunks of {}.{} and is written only by writes to {}",
            collection.name, parent.collection, parent.field_name, parent.collection
        ))),
        None => Ok(()),
    }
}
