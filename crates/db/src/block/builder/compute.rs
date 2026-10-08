use super::*;
use bytes::Bytes;

/// Pre-computed blocks ready for batch insertion into storage.
///
/// All (key, value) pairs are accumulated during pure computation (no storage
/// access). The caller inserts them into blockstore/headstore inside a transaction.
#[derive(Debug, Clone)]
pub struct ComputedBlocks {
    pub blockstore_entries: Vec<(Vec<u8>, Bytes)>,
    pub headstore_entries: Vec<(Vec<u8>, Vec<u8>)>,
    pub block_result: BlockResult,
}

/// Compute every block of a document create without touching storage.
///
/// For each field: CBOR encode -> optional encrypt -> build Block -> optional
/// sign -> serialize -> CID. Then the composite block the same way. Every key
/// is resolved beforehand (`resolve_document_keys`), so this is a pure function
/// of its inputs: no field block exists anywhere until the whole document,
/// and its DocID, is known. The public DocID is derived from the genesis
/// composite CID and returned in `block_result.doc_id`.
pub fn compute_document_blocks(
    doc: &Document,
    schema_version_id: &str,
    identity: DocStorageIdentity,
    keys: &DocumentKeys,
    signing_config: Option<&SigningConfig>,
) -> Result<ComputedBlocks, String> {
    let mut blockstore_entries: Vec<(Vec<u8>, Bytes)> = Vec::new();
    let mut headstore_entries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut field_links: Vec<DAGLink> = Vec::new();
    let mut field_cids: Vec<Cid> = Vec::new();
    let mut encryption_cids: Vec<Cid> = Vec::new();

    let priority: u64 = 1; // Always 1 for creates

    for (field_name, field_value) in doc.values() {
        if field_name == "_docID" {
            continue;
        }

        // For counter fields during creates, use the raw delta if set
        let cbor_value = if let Some(delta) = doc.get_counter_delta(field_name) {
            delta
        } else {
            field_value.value()
        };

        let value_bytes = encode_value_as_cbor(cbor_value)?;

        let (value_bytes, encryption_cid) = match keys.fields.get(field_name) {
            Some(key) => {
                if let Some(block) = &key.block {
                    blockstore_entries.push((key.cid.to_bytes(), block.clone()));
                }
                encryption_cids.push(key.cid);
                (encrypt_delta(&value_bytes, &key.key)?, Some(key.cid))
            }
            None => (value_bytes, None),
        };

        let is_counter = doc
            .fields()
            .get(field_name)
            .map(|f| f.crdt_type().is_counter())
            .unwrap_or(false)
            || doc.get_counter_delta(field_name).is_some();

        // For creates, heads are always empty -> nonce is always 0
        let nonce: i64 = 0;

        let delta = if is_counter {
            CrdtDelta::Counter(CounterDeltaPayload {
                field_name: field_name.clone(),
                priority,
                nonce,
                schema_version_id: schema_version_id.to_string(),
                data: value_bytes,
            })
        } else {
            CrdtDelta::Lww(LwwDeltaPayload {
                field_name: field_name.clone(),
                priority,
                schema_version_id: schema_version_id.to_string(),
                data: value_bytes,
            })
        };

        // No prev heads for creates
        let mut field_block = Block::new_with_options(delta, vec![], vec![], encryption_cid, None);

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

        // Head entry: /d/{doc_short_id}/{field_name}/{cid} -> priority
        let head_key = HeadstoreDocKey::new(identity.doc_short_id, field_name, field_cid);
        let priority_bytes = encode_priority_varint(priority);
        headstore_entries.push((head_key.bytes(), priority_bytes));
        headstore_entries.push((
            priority_index_key(identity.doc_short_id, priority, field_cid),
            vec![],
        ));

        field_links.push(DAGLink::new(field_name.clone(), field_cid));
        field_cids.push(field_cid);
    }

    let composite_encryption_cid = keys.composite.as_ref().map(|key| {
        if let Some(block) = &key.block {
            blockstore_entries.push((key.cid.to_bytes(), block.clone()));
        }
        encryption_cids.push(key.cid);
        key.cid
    });

    let composite_payload = CompositeDeltaPayload {
        schema_version_id: schema_version_id.to_string(),
        priority,
        status: 1,
    };

    let mut composite_block = Block::new_with_options(
        CrdtDelta::Composite(composite_payload),
        vec![],
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

    let composite_head_key = HeadstoreDocKey::new(identity.doc_short_id, "C", composite_cid);
    let priority_bytes = encode_priority_varint(priority);
    headstore_entries.push((composite_head_key.bytes(), priority_bytes));
    headstore_entries.push((
        priority_index_key(identity.doc_short_id, priority, composite_cid),
        vec![],
    ));

    Ok(ComputedBlocks {
        blockstore_entries,
        headstore_entries,
        block_result: BlockResult {
            cid: composite_cid,
            block: composite_bytes.into(),
            doc_id: derive_doc_id(&composite_cid),
            field_cids,
            encryption_cids,
        },
    })
}

/// Batch-insert pre-computed blocks into blockstore and headstore.
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
    for (key, value) in &blocks.headstore_entries {
        headstore
            .set(key, value)
            .await
            .map_err(|e| format!("Failed to write head: {}", e))?;
    }
    Ok(())
}
