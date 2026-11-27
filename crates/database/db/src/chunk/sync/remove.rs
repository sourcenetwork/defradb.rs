//! Deletes one chunk document, for a parent's write and for a merge alike.

use datastore::NamespaceView;
use document::DocID;
use query::error::{QueryError, Result};

use crate::block::builder::{write_delete_block_with_inputs, BlockResult};

use super::{json, ChunkCollection};

pub(crate) struct Removed {
    pub(crate) doc_id: DocID,
    pub(crate) block: BlockResult,
    pub(crate) before: Option<serde_json::Value>,
}

/// Deletes the chunk `doc_short_id` if live, unsigned so every node writes the same block.
pub(crate) async fn remove(
    target: &ChunkCollection,
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc_short_id: u64,
) -> Result<Option<Removed>> {
    let collection = &target.collection;
    let Some(doc_id) = crate::docid::map::get_doc_id(&target.systemstore, doc_short_id)
        .await
        .map_err(|e| QueryError::execution(e.to_string()))?
    else {
        tracing::warn!(doc_short_id, chunks = %collection.name(), "chunk index names a document with no DocID");
        return Ok(None);
    };
    let doc_id = DocID::from_string(&doc_id)
        .map_err(|e| QueryError::execution(format!("invalid chunk DocID: {e}")))?;
    let before = collection
        .get_with_datastore(&target.datastore, doc_short_id, &doc_id)
        .await
        .map_err(|e| QueryError::execution(format!("chunk read failed: {e}")))?
        .and_then(|doc| json(&doc));
    let existed = collection
        .delete_with_indexes(
            &target.datastore,
            &doc_id,
            doc_short_id,
            &target.index_manager,
        )
        .await
        .map_err(|e| QueryError::execution(format!("chunk delete error: {e}")))?;
    if !existed {
        return Ok(None);
    }
    let block = write_delete_block_with_inputs(
        blockstore,
        headstore,
        &doc_id.to_string(),
        doc_short_id,
        collection.version_id(),
        None,
        None,
        None,
        None,
    )
    .await
    .map_err(|error| {
        error.into_query_error_in(&format!(
            "failed to write chunk delete block on collection {}",
            collection.name()
        ))
    })?;
    crate::docid::map::set_block_doc_id_mapping(
        &target.systemstore,
        &block.cid.to_string(),
        &doc_id.to_string(),
    )
    .await
    .map_err(|e| QueryError::execution(e.to_string()))?;
    Ok(Some(Removed {
        doc_id,
        block,
        before,
    }))
}
