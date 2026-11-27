//! The chunks a document sync of a parent sends with it.

use document::{DocID, NormalValue};
use storage::corekv::Store;

use crate::chunk::hash::distinct;
use crate::chunk::split::split;
use crate::chunk::sync::ChunkCollection;
use crate::database::DB;
use crate::error::Result;
use crate::txn::DbTxn;

/// The DocIDs of the current chunks of `doc_id` a peer can't make: with vectors or encrypted.
pub(crate) async fn current_chunk_ids<S>(
    db: &DB<S>,
    txn: &DbTxn<S>,
    doc_id: &str,
) -> Result<Vec<String>>
where
    S: Store + 'static,
{
    let systemstore = txn.systemstore()?;
    let Some(doc_ref) = crate::docid::map::get_doc_ref(&systemstore, doc_id).await? else {
        return Ok(Vec::new());
    };
    let Some(parent) = db.find_collection_by_root_id(doc_ref.collection_short_id) else {
        return Ok(Vec::new());
    };
    if parent.schema().chunked_fields.is_empty() {
        return Ok(Vec::new());
    }
    let Some(doc) = parent
        .get_with_datastore(
            &txn.datastore()?,
            doc_ref.doc_short_id,
            &DocID::from_string(doc_id)?,
        )
        .await?
    else {
        return Ok(Vec::new());
    };
    let mut ids = Vec::new();
    for chunked in &parent.schema().chunked_fields {
        let Some(text) = doc.get(&chunked.field_name).and_then(NormalValue::as_str) else {
            continue;
        };
        let target = ChunkCollection::open(txn, &chunked.collection(parent.name())).await?;
        // A peer's merge makes the others itself, under the same IDs.
        if target.collection.schema().vector_embeddings.is_empty()
            && !encrypts(txn, doc_ref, &chunked.field_name).await?
        {
            continue;
        }
        let config = crate::chunk::chunk_config(parent.name(), chunked)?;
        let chunks = split(text, &config);
        for chunk in distinct(&chunks) {
            for short_id in target.chunks_with(doc_id, &chunk.hash).await? {
                match crate::docid::map::get_doc_id(&systemstore, short_id).await? {
                    Some(chunk_id) => ids.push(chunk_id),
                    None => tracing::warn!(
                        chunks = %target.collection.name(),
                        doc_short_id = short_id,
                        "chunk index names a document with no DocID"
                    ),
                }
            }
        }
    }
    Ok(ids)
}

async fn encrypts<S>(
    txn: &DbTxn<S>,
    parent: storage::keys::doc_id_index::DocRef,
    field: &str,
) -> Result<bool>
where
    S: Store + 'static,
{
    let (encrypt_doc, fields) = crate::block::builder::inherited_encryption_policy(
        &txn.blockstore()?,
        &txn.headstore()?,
        parent,
        &[field.to_string()],
    )
    .await
    .map_err(crate::error::Error::Other)?;
    Ok(encrypt_doc || !fields.is_empty())
}
