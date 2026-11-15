//! Refuses user writes to what only a parent's own writes may set.

use document::Document;
use query::error::{QueryError, Result};
use rapidhash::RapidHashSet;
use schema::CollectionVersion;

use crate::error::Error;

/// Refuses a user write to a chunk collection. Only the parent's writes change chunks.
pub(crate) fn ensure_user_writable(collection: &CollectionVersion) -> Result<()> {
    match &collection.chunk_of {
        Some(parent) => Err(QueryError::execution(format!(
            "{} holds the chunks of {}.{} and is written only by writes to {}",
            collection.name, parent.collection, parent.field_name, parent.collection
        ))),
        None => Ok(()),
    }
}

/// Refuses an admin change to a chunk collection, such as an index, a truncate or a
/// version switch. The parent's writes rely on the generated definition as it is.
pub fn ensure_admin_changeable(collection: &CollectionVersion) -> crate::error::Result<()> {
    match &collection.chunk_of {
        Some(parent) => Err(Error::ChunkCollection {
            collection: collection.name.clone(),
            parent: parent.collection.clone(),
            field: parent.field_name.clone(),
        }),
        None => Ok(()),
    }
}

/// Refuses activating `target` unless it keeps the `@chunk` settings of its
/// collection's first version. `other` is any other version of that collection.
pub(crate) fn ensure_keeps_first_chunk_settings(
    target: &CollectionVersion,
    other: &CollectionVersion,
) -> crate::error::Result<()> {
    if other.previous_version.is_some() || other.is_placeholder {
        return Ok(());
    }
    Ok(schema::ensure_chunk_settings_kept(other, target)?)
}

/// Refuses deleting a chunk collection while a version of its parent stays.
/// `is_deleted` tells which of `versions` this delete removes.
pub(crate) fn ensure_removed_with_parent<F>(
    versions: &[CollectionVersion],
    is_deleted: F,
) -> crate::error::Result<()>
where
    F: Fn(&CollectionVersion) -> bool,
{
    for version in versions {
        let Some(parent) = &version.chunk_of else {
            continue;
        };
        let parent_stays = versions
            .iter()
            .any(|other| other.name == parent.collection && !is_deleted(other));
        if is_deleted(version) && parent_stays {
            ensure_admin_changeable(version)?;
        }
    }
    Ok(())
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
