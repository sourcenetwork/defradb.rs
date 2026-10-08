use super::plan::inherited_encryption_cid;
use super::*;

/// Write a document's blocks and heads: plan, compute, then insert.
///
/// `modified_fields` is `None` for a create, which gets a block for every
/// field; an update gets blocks for the modified fields only. No block is
/// stored until every block of the write is computed.
#[allow(clippy::too_many_arguments)]
pub async fn write_document_blocks(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc: &Document,
    schema_version_id: &str,
    identity: DocStorageIdentity,
    modified_fields: Option<&rapidhash::RapidHashSet<String>>,
    encryption_config: Option<&EncryptionConfig>,
    signing_config: Option<&SigningConfig>,
    kms: Option<&std::sync::Arc<dyn kms::KmsService>>,
) -> Result<BlockResult, String> {
    let plan = plan_document_blocks(
        blockstore,
        headstore,
        doc,
        identity,
        modified_fields,
        encryption_config,
        kms,
    )
    .await?;
    let blocks = compute_document_blocks(doc, schema_version_id, identity, &plan, signing_config)?;
    insert_computed_blocks(blockstore, headstore, &blocks).await?;
    Ok(blocks.block_result)
}

/// Write a delete block (composite with status=2) to blockstore and heads to headstore.
pub async fn write_delete_block(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc_id: &str,
    doc_short_id: u64,
    schema_version_id: &str,
    signing_config: Option<&SigningConfig>,
) -> Result<BlockResult, String> {
    let snapshot = DocHeadsSnapshot::load(headstore, doc_short_id).await?;
    let priority: u64 = snapshot.max_priority() + 1;

    let composite_head_entries = snapshot.field_heads("C");
    let composite_heads: Vec<Cid> = composite_head_entries.iter().map(|h| h.cid).collect();

    let composite_payload = CompositeDeltaPayload {
        schema_version_id: schema_version_id.to_string(),
        priority,
        status: 2,
    };

    let composite_encryption_cid = inherited_encryption_cid(blockstore, &composite_heads).await?;

    let mut composite_block = Block::new_with_options(
        CrdtDelta::Composite(composite_payload),
        composite_heads,
        vec![],
        composite_encryption_cid,
        None,
    );

    if let Some(signer) = signing_config {
        if let Some(sig_cid) = sign_block(&composite_block, signer, blockstore).await? {
            composite_block.signature = Some(sig_cid);
        }
    }

    let composite_bytes = composite_block
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode delete composite block: {}", e))?;
    let composite_cid = generate_cid_from_bytes(&composite_bytes)
        .map_err(|e| format!("Failed to generate delete composite CID: {}", e))?;

    blockstore
        .set(&composite_cid.to_bytes(), &composite_bytes)
        .await
        .map_err(|e| format!("Failed to store delete composite block: {}", e))?;

    for old_head in &composite_head_entries {
        headstore
            .delete(&old_head.key)
            .await
            .map_err(|e| format!("Failed to delete old composite head: {}", e))?;
    }

    let composite_head_key = HeadstoreDocKey::new(doc_short_id, "C", composite_cid);
    let priority_bytes = encode_priority_varint(priority);
    headstore
        .set(&composite_head_key.bytes(), &priority_bytes)
        .await
        .map_err(|e| format!("Failed to write delete composite head: {}", e))?;
    headstore
        .set(
            &priority_index_key(doc_short_id, priority, composite_cid),
            &[],
        )
        .await
        .map_err(|e| format!("Failed to write delete priority index: {}", e))?;

    tracing::debug!(
        doc_id = %doc_id,
        cid = %composite_cid,
        priority = priority,
        "Built delete composite block (status=2)"
    );

    if let Some(session_key) = defra_core::batch_signing::get_batch_session_key() {
        defra_core::batch_signing::batch_collect_cid(&session_key, composite_cid);
    }

    Ok(BlockResult {
        cid: composite_cid,
        block: composite_bytes.into(),
        doc_id: doc_id.to_string(),
        field_cids: vec![],
        encryption_cids: vec![],
    })
}
