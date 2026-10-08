use super::*;
use bytes::Bytes;

/// Pre-computed blocks ready for insertion into storage.
///
/// Built without touching storage; [`insert_computed_blocks`] applies them
/// inside a transaction.
#[derive(Debug, Clone)]
pub struct ComputedBlocks {
    pub blockstore_entries: Vec<(Vec<u8>, Bytes)>,
    pub headstore_entries: Vec<(Vec<u8>, Vec<u8>)>,
    /// Head keys the new blocks supersede.
    pub stale_heads: Vec<Vec<u8>>,
    /// Blocks a batch signing session signs: new genesis field blocks and the
    /// composite.
    pub batch_signed: Vec<Cid>,
    pub block_result: BlockResult,
}

/// Compute every block of a document write without touching storage.
///
/// For each planned field: CBOR encode -> optional encrypt -> build Block ->
/// optional sign -> serialize -> CID. Then the composite block the same way.
/// Everything that needs storage or a KMS is resolved beforehand
/// ([`plan_document_blocks`](super::plan_document_blocks)), so no block exists
/// anywhere until the whole document is known. A create derives the public
/// DocID from its genesis composite CID; an update keeps the document's.
pub fn compute_document_blocks(
    doc: &Document,
    schema_version_id: &str,
    identity: DocStorageIdentity,
    plan: &BlockPlan,
    signing_config: Option<&SigningConfig>,
) -> Result<ComputedBlocks, String> {
    let mut blockstore_entries: Vec<(Vec<u8>, Bytes)> = Vec::new();
    let mut headstore_entries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut stale_heads: Vec<Vec<u8>> = Vec::new();
    let mut batch_signed: Vec<Cid> = Vec::new();
    let mut field_links: Vec<DAGLink> = Vec::new();
    let mut field_cids: Vec<Cid> = Vec::new();
    let mut encryption_cids: Vec<Cid> = Vec::new();

    let mut link_key = |link: &KeyLink, entries: &mut Vec<(Vec<u8>, Bytes)>| {
        if let Some(block) = &link.block {
            entries.push((link.cid.to_bytes(), block.clone()));
        }
        if link.minted {
            encryption_cids.push(link.cid);
        }
        link.cid
    };

    for (field_name, field_value) in doc.values() {
        let Some(field) = plan.fields.get(field_name) else {
            continue;
        };
        let position = &field.position;

        let cbor_value = doc
            .get_counter_delta(field_name)
            .unwrap_or_else(|| field_value.value());
        let value_bytes = encode_value_as_cbor(cbor_value)?;

        let (value_bytes, encryption_cid) = match &field.key {
            Some(key) => (
                encrypt_delta(&value_bytes, &key.key)?,
                Some(link_key(&key.link, &mut blockstore_entries)),
            ),
            None => (value_bytes, None),
        };

        let is_counter = doc
            .fields()
            .get(field_name)
            .is_some_and(|f| f.crdt_type().is_counter())
            || doc.get_counter_delta(field_name).is_some();

        let delta = if is_counter {
            CrdtDelta::Counter(CounterDeltaPayload {
                field_name: field_name.clone(),
                priority: position.priority,
                nonce: field.nonce,
                schema_version_id: schema_version_id.to_string(),
                data: value_bytes,
            })
        } else {
            CrdtDelta::Lww(LwwDeltaPayload {
                field_name: field_name.clone(),
                priority: position.priority,
                schema_version_id: schema_version_id.to_string(),
                data: value_bytes,
            })
        };

        let mut field_block =
            Block::new_with_options(delta, position.heads.clone(), vec![], encryption_cid, None);
        if let Some(signer) = signing_config {
            if let Some((sig_cid, sig_cbor)) = compute_signature(&field_block, signer)? {
                blockstore_entries.push((sig_cid.to_bytes(), sig_cbor));
                field_block.signature = Some(sig_cid);
            }
        }

        let field_block_bytes = field_block
            .to_dag_cbor()
            .map_err(|e| format!("Failed to encode field block: {}", e))?;
        let field_cid = generate_cid_from_bytes(&field_block_bytes)
            .map_err(|e| format!("Failed to generate field CID: {}", e))?;

        blockstore_entries.push((field_cid.to_bytes(), field_block_bytes.into()));
        push_head(
            &mut headstore_entries,
            identity.doc_short_id,
            field_name,
            position.priority,
            field_cid,
        );
        stale_heads.extend(position.stale_heads.iter().cloned());
        if position.priority == 1 {
            batch_signed.push(field_cid);
        }

        field_links.push(DAGLink::new(field_name.clone(), field_cid));
        field_cids.push(field_cid);
    }

    let composite_encryption_cid = plan
        .composite_key
        .as_ref()
        .map(|link| link_key(link, &mut blockstore_entries));

    let composite = &plan.composite;
    let mut composite_block = Block::new_with_options(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: schema_version_id.to_string(),
            priority: composite.priority,
            status: 1,
        }),
        composite.heads.clone(),
        field_links,
        composite_encryption_cid,
        None,
    );
    if let Some(signer) = signing_config {
        if let Some((sig_cid, sig_cbor)) = compute_signature(&composite_block, signer)? {
            blockstore_entries.push((sig_cid.to_bytes(), sig_cbor));
            composite_block.signature = Some(sig_cid);
        }
    }

    let composite_bytes = composite_block
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode composite block: {}", e))?;
    let composite_cid = generate_cid_from_bytes(&composite_bytes)
        .map_err(|e| format!("Failed to generate composite CID: {}", e))?;

    blockstore_entries.push((composite_cid.to_bytes(), composite_bytes.clone().into()));
    push_head(
        &mut headstore_entries,
        identity.doc_short_id,
        "C",
        composite.priority,
        composite_cid,
    );
    stale_heads.extend(composite.stale_heads.iter().cloned());
    batch_signed.push(composite_cid);

    let doc_id = if plan.is_create {
        derive_doc_id(&composite_cid)
    } else {
        doc.id()
            .ok_or_else(|| "Document must have an ID for updates".to_string())?
            .to_string()
    };

    Ok(ComputedBlocks {
        blockstore_entries,
        headstore_entries,
        stale_heads,
        batch_signed,
        block_result: BlockResult {
            cid: composite_cid,
            block: composite_bytes.into(),
            doc_id,
            field_cids,
            encryption_cids,
        },
    })
}

/// Head entry `/d/{doc_short_id}/{field}/{cid} -> priority`, plus its priority index.
fn push_head(
    entries: &mut Vec<(Vec<u8>, Vec<u8>)>,
    doc_short_id: u64,
    field: &str,
    priority: u64,
    cid: Cid,
) {
    let head_key = HeadstoreDocKey::new(doc_short_id, field, cid);
    entries.push((head_key.bytes(), encode_priority_varint(priority)));
    entries.push((priority_index_key(doc_short_id, priority, cid), vec![]));
}

/// Insert pre-computed blocks into blockstore and headstore, retiring the heads
/// they supersede.
pub async fn insert_computed_blocks(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    blocks: &ComputedBlocks,
) -> Result<(), String> {
    for (key, value) in &blocks.blockstore_entries {
        blockstore
            .set(key, value)
            .await
            .map_err(|e| format!("Failed to store block: {}", e))?;
    }
    for key in &blocks.stale_heads {
        headstore
            .delete(key)
            .await
            .map_err(|e| format!("Failed to delete superseded head: {}", e))?;
    }
    for (key, value) in &blocks.headstore_entries {
        headstore
            .set(key, value)
            .await
            .map_err(|e| format!("Failed to write head: {}", e))?;
    }
    if let Some(session_key) = defra_core::batch_signing::get_batch_session_key() {
        for cid in &blocks.batch_signed {
            defra_core::batch_signing::batch_collect_cid(&session_key, *cid);
        }
    }
    Ok(())
}
