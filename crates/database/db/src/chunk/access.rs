//! A chunk's blocks and keys follow its parent's permissions, old chunks too.

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
    txn: &DbTxn<S>,
    chunks_col: &CollectionVersion,
    chunk_ids: &[String],
) -> Result<Vec<String>>
where
    S: Store,
{
    let collection = db
        .get_collection(&chunks_col.name)?
        .ok_or_else(|| Error::CollectionNotFound(chunks_col.name.clone()))?;
    let parents = parents_of_chunks(txn, &collection, chunk_ids).await?;
    merged(txn, parents).await
}

/// The collection whose readers decide a chunk collection's reads.
///
/// `None` when it is missing or doesn't chunk the field the chunks name, so the caller denies.
pub fn parent_collection<S>(
    db: &DB<S>,
    chunks_col: &CollectionVersion,
) -> Result<Option<Collection>>
where
    S: Store,
{
    let Some(source) = &chunks_col.chunk_of else {
        return Ok(None);
    };
    Ok(db
        .get_collection(&source.collection)?
        .filter(|parent| parent.schema().chunked_field(&source.field_name).is_some()))
}

/// Each chunk replaced by its parent, so the parent's readers decide.
pub async fn read_owners<S>(db: &DB<S>, txn: &DbTxn<S>, doc_ids: Vec<String>) -> Result<Vec<String>>
where
    S: Store,
{
    let systemstore = txn.systemstore()?;
    let mut owners = Vec::with_capacity(doc_ids.len());
    for doc_id in doc_ids {
        // A document of an unknown collection might be a chunk, which has no policy of its own.
        let Some(doc_ref) = crate::docid::map::get_doc_ref(&systemstore, &doc_id).await? else {
            continue;
        };
        let Some(collection) = db.find_collection_by_root_id(doc_ref.collection_short_id) else {
            if stored_as_plain(txn, &doc_id).await? {
                owners.push(doc_id);
            }
            continue;
        };
        if !collection.schema().is_generated() {
            owners.push(doc_id);
            continue;
        }
        let parents = parents_of_chunks(txn, &collection, &[doc_id]).await?;
        owners.extend(merged(txn, parents).await?);
    }
    Ok(owners)
}

// The cache can miss a collection the store holds, such as a synced one whose name another holds.
async fn stored_as_plain<S>(txn: &DbTxn<S>, doc_id: &str) -> Result<bool>
where
    S: Store,
{
    let found = crate::collection::retriever::stored_doc_collection(
        &txn.headstore()?,
        &txn.blockstore()?,
        &txn.systemstore()?,
        doc_id,
    )
    .await?;
    Ok(found.is_some_and(|collection| !collection.is_generated()))
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
