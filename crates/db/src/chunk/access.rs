//! Whose permissions decide access to a chunk's blocks and keys.
//!
//! A chunk has no policy, so its parent's applies. Blocks are history, so old
//! chunks and chunks of a deleted parent count too, and they replicate like
//! the parent's own old versions.

use document::{DocID, NormalValue};
use schema::{CollectionVersion, CHUNK_SOURCE_ID_FIELD};
use storage::corekv::Store;

use crate::collection::Collection;
use crate::database::DB;
use crate::error::{Error, Result};
use crate::txn::DbTxn;

/// The parents of the chunks in `chunk_ids` that this node has merged.
///
/// A chunk whose parent hasn't arrived has no owner, so it is refused, not served as public.
pub async fn history_parents<S>(
    db: &DB<S>,
    chunks_col: &CollectionVersion,
    chunk_ids: &[String],
) -> Result<Vec<String>>
where
    S: Store,
{
    let collection = db
        .get_collection(&chunks_col.name)?
        .ok_or_else(|| Error::CollectionNotFound(chunks_col.name.clone()))?;
    let txn = db.new_txn(true).await?;
    let found = match parents_of_chunks(&txn, &collection, chunk_ids).await {
        Ok(parents) => merged(&txn, parents).await,
        Err(error) => Err(error),
    };
    let _ = txn.discard();
    found
}

/// `doc_ids` with each chunk replaced by its parent, whose permissions decide.
///
/// A parent this node hasn't merged yet is left out, as in [`history_parents`].
pub async fn read_owners<S>(db: &DB<S>, txn: &DbTxn<S>, doc_ids: Vec<String>) -> Result<Vec<String>>
where
    S: Store,
{
    let systemstore = txn.systemstore()?;
    let mut owners = Vec::with_capacity(doc_ids.len());
    for doc_id in doc_ids {
        let doc_ref = crate::docid::map::get_doc_ref(&systemstore, &doc_id).await?;
        let chunks_col = doc_ref
            .and_then(|r| db.find_collection_by_root_id(r.collection_short_id))
            .filter(|collection| collection.schema().is_generated());
        let Some(chunks_col) = chunks_col else {
            owners.push(doc_id);
            continue;
        };
        let parents = parents_of_chunks(txn, &chunks_col, &[doc_id]).await?;
        owners.extend(merged(txn, parents).await?);
    }
    Ok(owners)
}

async fn merged<S>(txn: &DbTxn<S>, parents: Vec<String>) -> Result<Vec<String>>
where
    S: Store,
{
    let systemstore = txn.systemstore()?;
    let mut found = Vec::with_capacity(parents.len());
    for parent in parents {
        if crate::docid::map::get_doc_ref(&systemstore, &parent)
            .await?
            .is_some()
        {
            found.push(parent);
        }
    }
    Ok(found)
}

// One parent per chunk, so chunks of the same parent repeat it.
async fn parents_of_chunks<S>(
    txn: &DbTxn<S>,
    chunks_col: &Collection,
    chunk_ids: &[String],
) -> Result<Vec<String>>
where
    S: Store,
{
    let datastore = txn.datastore()?;
    let systemstore = txn.systemstore()?;
    let mut parents = Vec::with_capacity(chunk_ids.len());
    for doc_id in chunk_ids {
        let Some(doc_ref) = crate::docid::map::get_doc_ref(&systemstore, doc_id).await? else {
            continue;
        };
        let found = chunks_col
            .get_with_datastore_include_deleted(
                &datastore,
                doc_ref.doc_short_id,
                &DocID::from_string(doc_id)?,
                false,
            )
            .await?;
        if let Some(NormalValue::String(parent)) = found
            .as_ref()
            .and_then(|(chunk, _)| chunk.get(CHUNK_SOURCE_ID_FIELD))
        {
            parents.push(parent.clone());
        }
    }
    Ok(parents)
}
