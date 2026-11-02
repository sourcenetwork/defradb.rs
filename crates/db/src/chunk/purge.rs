//! Purging a parent's `@chunk` chunks with it.

use storage::corekv::Store;
use storage::index::IndexIterator;

use crate::collection::Collection;
use crate::error::{Error, Result};
use crate::txn::DbTxn;

/// The `@chunk` collections of `parent`, read in `txn`.
pub(crate) async fn collections<S>(txn: &DbTxn<S>, parent: &Collection) -> Result<Vec<Collection>>
where
    S: Store,
{
    let mut collections = Vec::new();
    for chunked in &parent.schema().chunked_fields {
        let name = chunked.collection(parent.name());
        collections.push(
            txn.get_collection(&name)
                .await?
                .ok_or(Error::CollectionNotFound(name))?,
        );
    }
    Ok(collections)
}

/// The DocIDs of every chunk of `parents`, old ones too, which the parents' lists no longer name.
pub(crate) async fn doc_ids<S>(
    txn: &DbTxn<S>,
    chunks_col: &Collection,
    parents: &[String],
) -> Result<Vec<document::DocID>>
where
    S: Store,
{
    let index_manager = crate::IndexManager::from_indexes(
        chunks_col.resolved_root_id(),
        chunks_col.schema(),
        chunks_col.write_indexes(),
    )?;
    let index = chunks_col
        .schema()
        .indexes
        .iter()
        .find(|index| {
            index
                .fields
                .first()
                .is_some_and(|field| field.name == schema::CHUNK_SOURCE_ID_FIELD)
        })
        .and_then(|index| index_manager.get_index(&index.name))
        .ok_or_else(|| Error::Other(format!("{} has no source index", chunks_col.name())))?;
    let datastore = txn.datastore()?;
    let systemstore = txn.systemstore()?;
    let mut doc_ids = Vec::new();
    for parent in parents {
        let mut entries = index
            .scan_prefix(
                &datastore,
                &[document::NormalValue::String(parent.clone())],
                false,
            )
            .await
            .map_err(Error::Storage)?;
        for entry in entries.collect_all().await.map_err(Error::Storage)? {
            match crate::docid::map::get_doc_id(&systemstore, entry.doc_short_id).await? {
                Some(doc_id) => doc_ids.push(document::DocID::from_string(&doc_id)?),
                None => tracing::warn!(
                    collection = chunks_col.name(),
                    doc_short_id = entry.doc_short_id,
                    "Chunk index entry has no DocID; leaving it"
                ),
            }
        }
    }
    Ok(doc_ids)
}
