use super::*;
use bytes::Bytes;

/// Write a collection-level block for branchable collections.
///
/// Branchable collections maintain a separate DAG at the collection level,
/// tracking all document operations. Each mutation creates a collection block
/// that links to the document's composite block.
///
/// This matches Go's `crdt.NewCollection()` → `coreblock.AddDelta()` flow.
pub async fn write_collection_block(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    collection_short_id: u32,
    collection_version_id: &str,
    doc_composite_cid: Cid,
    signing_config: Option<&SigningConfig>,
) -> Result<(Cid, Bytes), String> {
    write_collection_block_with_inputs(
        blockstore,
        headstore,
        collection_short_id,
        collection_version_id,
        doc_composite_cid,
        signing_config,
        None,
        None,
    )
    .await
    .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_collection_block_with_inputs(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    collection_short_id: u32,
    collection_version_id: &str,
    doc_composite_cid: Cid,
    signing_config: Option<&SigningConfig>,
    batch_session: Option<&str>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<(Cid, Bytes), BuildError> {
    let set = crate::block::heads::collection_set(headstore, collection_short_id)
        .await
        .map_err(|e| format!("Failed to read collection heads: {}", e))?;
    // Live names ascend by CID text, as Go's Block.New() sorts the heads.
    let found = crate::block::heads::CollectionHeads::of(&set);
    let priority: u64 = found.max_priority + 1;
    let col_heads = found.live;

    // Create the Collection delta payload
    let collection_payload = CollectionDeltaPayload {
        collection_version_id: collection_version_id.to_string(),
        priority,
    };

    // The collection block links to the document composite block
    // Go uses empty string for link name here, fieldName comes from linked block's delta
    let links = vec![DAGLink::new(String::new(), doc_composite_cid)];

    // Create the collection block
    let mut collection_block =
        Block::new(CrdtDelta::Collection(collection_payload), col_heads, links);

    // Sign the collection block if a signer is available
    if let Some(signer) = signing_config {
        if let Some(sig_cid) =
            sign_block(&collection_block, signer, blockstore, prepared_inputs).await?
        {
            collection_block.signature = Some(sig_cid);
        }
    }

    // Serialize and generate CID
    let collection_bytes = collection_block
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode collection block: {}", e))?;
    let collection_cid = generate_cid_from_bytes(&collection_bytes)
        .map_err(|e| format!("Failed to generate collection CID: {}", e))?;

    // Store the collection block in blockstore
    blockstore
        .set(&cid_bytes(&collection_cid), &collection_bytes)
        .await
        .map_err(|e| format!("Failed to store collection block: {}", e))?;

    // One blind merge: this block is a head and its parents are not. Two
    // writers appending at once both commit.
    crate::block::heads::publish_collection_block(
        headstore,
        collection_short_id,
        &set,
        collection_cid,
        priority,
        collection_block.heads.as_deref().unwrap_or_default(),
    )
    .await
    .map_err(|e| format!("Failed to write collection head: {}", e))?;

    tracing::debug!(
        collection_id = collection_short_id,
        cid = %collection_cid,
        priority = priority,
        doc_composite_cid = %doc_composite_cid,
        "Built collection block for branchable collection"
    );

    collect_on_commit(
        blockstore,
        batch_session,
        [(collection_cid, defra_core::batch_signing::BatchCid::Other)],
    )
    .await?;

    Ok((collection_cid, collection_bytes.into()))
}
