//! Refuses user writes to what only a parent's own writes may set.

use document::Document;
use query::error::{QueryError, Result};
use rapidhash::RapidHashSet;
use schema::CollectionVersion;

/// Refuses a user write to a chunk collection. Only the parent's writes change chunks.
pub(crate) fn ensure_user_writable(collection: &CollectionVersion) -> Result<()> {
    match &collection.chunk_of {
        Some(source) => Err(QueryError::execution(format!(
            "{} holds the chunks of {}.{} and is written only by writes to {}",
            collection.name, source.collection, source.field_name, source.collection
        ))),
        None => Ok(()),
    }
}

/// Refuses a write that sets a chunk list itself, because lists come only
/// from the text. `modified` is `None` for a create.
pub(crate) fn refuse_list_writes(
    schema: &CollectionVersion,
    doc: &Document,
    modified: Option<&RapidHashSet<String>>,
) -> Result<()> {
    for chunked in &schema.chunked_fields {
        let list_field = chunked.list_field();
        let set_by_user = match modified {
            Some(modified) => modified.contains(&list_field),
            None => doc.get(&list_field).is_some(),
        };
        if set_by_user {
            return Err(QueryError::execution(format!(
                "{}.{list_field} is read-only: it is set from {} by every write that changes it",
                schema.name, chunked.field_name
            )));
        }
    }
    Ok(())
}
