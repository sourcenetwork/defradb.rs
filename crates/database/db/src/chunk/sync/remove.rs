//! Deletes one chunk document, for a parent's write and for a merge alike.

use datastore::NamespaceView;
use defra_core::signing::SigningConfig;
use document::DocID;
use query::error::{QueryError, Result};
use query::prepared::PreparedInputMemo;

use crate::block::builder::{write_delete_block_with_inputs, BlockResult};

use super::{json, ChunkCollection};

/// A chunk document that was deleted, and what it held before.
pub(crate) struct Removed {
    pub(crate) doc_id: DocID,
    pub(crate) block: BlockResult,
    pub(crate) before: Option<serde_json::Value>,
}

/// How the delete block is written.
#[derive(Default)]
pub(crate) struct RemoveInputs<'a> {
    pub(crate) signing: Option<&'a SigningConfig>,
    pub(crate) batch_session: Option<&'a str>,
    pub(crate) prepared_inputs: Option<&'a PreparedInputMemo>,
}

/// Deletes the chunk document `doc_short_id`, or does nothing when it is gone.
pub(crate) async fn remove(
    target: &ChunkCollection,
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc_short_id: u64,
    inputs: RemoveInputs<'_>,
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
        inputs.signing,
        inputs.batch_session,
        inputs.prepared_inputs,
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
