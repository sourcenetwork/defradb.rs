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
    /// The document's new heads, merged into its head set once.
    pub head_write: storage::merge::head_set::Write,
    pub doc_short_id: u64,
    pub block_result: BlockResult,
}

/// Compute all document blocks without any storage access.
///
/// This is the CREATE-only path extracted from `write_document_blocks()`.
/// For creates: priority is always 1, no headstore reads, no prev heads.
/// The entire computation is a pure function of the inputs.
///
/// For each field: CBOR encode -> optional encrypt -> build Block -> optional sign ->
/// serialize -> CID. Then builds composite block the same way. The public
/// DocID is derived from the genesis composite block CID and returned in
/// `block_result.doc_id`; the caller persists the short-ID mappings.
/// Accumulates all (key, value) pairs instead of writing to storage.
pub fn compute_document_blocks(
    crdts: &crdt::Crdts,
    doc: &Document,
    schema: &CollectionVersion,
    identity: DocStorageIdentity,
    encryption_config: Option<&EncryptionConfig>,
    signing_config: Option<&SigningConfig>,
) -> Result<ComputedBlocks, String> {
    compute_document_blocks_with_inputs(
        crdts,
        doc,
        schema,
        identity,
        encryption_config,
        signing_config,
        None,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn compute_document_blocks_with_inputs(
    crdts: &crdt::Crdts,
    doc: &Document,
    schema: &CollectionVersion,
    identity: DocStorageIdentity,
    encryption_config: Option<&EncryptionConfig>,
    signing_config: Option<&SigningConfig>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<ComputedBlocks, BuildError> {
    let schema_version_id = schema.version_id.as_str();
    let doc_ref_bytes = identity.doc_ref_bytes();

    let field_count = doc
        .values()
        .len()
        .saturating_sub(usize::from(doc.values().contains_key("_docID")));
    let document_and_field_count = field_count.saturating_add(1);
    let mut blockstore_entries: Vec<(Vec<u8>, Bytes)> =
        Vec::with_capacity(document_and_field_count);
    let mut headstore_entries: Vec<(Vec<u8>, Vec<u8>)> =
        Vec::with_capacity(document_and_field_count.saturating_mul(3));
    let mut head_write = storage::merge::head_set::Write::default();
    let mut field_links: Vec<DAGLink> = Vec::with_capacity(field_count);
    let mut field_cids: Vec<Cid> = Vec::with_capacity(field_count);
    let mut encryption_cids: Vec<Cid> = Vec::new();

    let priority: u64 = 1; // Always 1 for creates
                           // The first input this pass is missing. The pass goes on past it, so
                           // every field's missing input is asked for in one wave.
    let mut first_miss = None;

    let (immutable_names, immutable) = immutable_for_pass(
        doc,
        schema,
        encryption_config,
        signing_config,
        prepared_inputs,
        &mut first_miss,
    )?;

    for (field_name, field_value) in doc.values() {
        if field_name == "_docID" || immutable_names.contains(field_name) {
            continue;
        }

        let crdt = field_crdt_of(crdts, doc, field_name)?;
        let Some(value_bytes) = crdt
            .block_data(field_value.value(), doc.pending_op(field_name))
            .map_err(|error| error.to_string())?
        else {
            continue;
        };

        // Encrypt delta and create Encryption metadata block if configured
        let (value_bytes, encryption_cid) = if let Some(enc) = encryption_config {
            if enc.should_encrypt_field(field_name) {
                let key_field_name = if enc.should_encrypt_individual_field(field_name) {
                    Some(field_name.as_str())
                } else {
                    None
                };
                let prepared_key = doc.write_preparation().and_then(|prepared| {
                    (prepared.key_scope_matches(field_name, key_field_name))
                        .then(|| prepared.key(field_name))
                        .flatten()
                });
                let (encrypted, enc_cid) = if let Some((cid, key)) = prepared_key {
                    let encrypted = match prepared_inputs {
                        Some(inputs) => encrypt_delta_prepared(&value_bytes, key, inputs)?,
                        None => encrypt_delta(&value_bytes, key)?,
                    };
                    (encrypted, cid)
                } else {
                    let needs_kms = doc
                        .write_preparation()
                        .is_some_and(|prepared| prepared.requires_kms_keys());
                    if needs_kms {
                        let inputs = prepared_inputs.ok_or_else(|| {
                            BuildError::Message(format!(
                                "encryption key for field {field_name} was not prepared"
                            ))
                        })?;
                        let request = crate::write::prepare::kms_input_request(
                            crate::write::prepare::PreparedKmsOperation::Generate {
                                doc_id: hex::encode(&doc_ref_bytes),
                                field: key_field_name.map(str::to_owned),
                            },
                        );
                        let Some(response) = inputs.get_or_want(&request) else {
                            first_miss.get_or_insert(request);
                            continue;
                        };
                        let response: crate::write::prepare::PreparedKmsResponse =
                            serde_json::from_slice(&response).map_err(|error| {
                                BuildError::Message(format!(
                                    "invalid cached KMS key response: {error}"
                                ))
                            })?;
                        let enc_cid = Cid::try_from(response.cid.as_str()).map_err(|error| {
                            BuildError::Message(format!(
                                "invalid CID in cached KMS key response: {error}"
                            ))
                        })?;
                        let key: [u8; 32] = response.key.try_into().map_err(|key: Vec<u8>| {
                            BuildError::Message(format!(
                                "KMS returned a {}-byte key for {enc_cid}, expected 32",
                                key.len()
                            ))
                        })?;
                        let encrypted = match prepared_inputs {
                            Some(inputs) => encrypt_delta_prepared(&value_bytes, &key, inputs)?,
                            None => encrypt_delta(&value_bytes, &key)?,
                        };
                        (encrypted, enc_cid)
                    } else {
                        let key = encryption_key_prepared(
                            &doc_ref_bytes,
                            key_field_name,
                            prepared_inputs,
                        )?;
                        let encrypted = match prepared_inputs {
                            Some(inputs) => encrypt_delta_prepared(&value_bytes, &key, inputs)?,
                            None => encrypt_delta(&value_bytes, &key)?,
                        };

                        let enc_block = Encryption { key: key.to_vec() };
                        let enc_bytes = enc_block
                            .to_dag_cbor()
                            .map_err(|e| format!("Failed to encode encryption block: {}", e))?;
                        let enc_cid = generate_cid_from_bytes(&enc_bytes)
                            .map_err(|e| format!("Failed to generate encryption CID: {}", e))?;
                        blockstore_entries.push((cid_bytes(&enc_cid), enc_bytes.into()));

                        (encrypted, enc_cid)
                    }
                };

                (encrypted, Some(enc_cid))
            } else {
                (value_bytes, None)
            }
        } else {
            (value_bytes, None)
        };

        if let Some(enc_cid) = encryption_cid {
            encryption_cids.push(enc_cid);
        }

        // A create's blocks take no nonce, so its first blocks are reproducible.
        let delta = crdt.delta(crdt::DeltaParts {
            field_name,
            priority,
            schema_version_id,
            nonce: 0,
            data: value_bytes,
        });

        // No prev heads for creates
        let mut field_block = Block::new_with_options(delta, vec![], vec![], encryption_cid, None);

        if let Some(signer) = signing_config {
            match compute_signature_prepared(&field_block, signer, prepared_inputs) {
                Ok(Some((sig_cid, sig_cbor))) => {
                    blockstore_entries.push((cid_bytes(&sig_cid), sig_cbor));
                    field_block.signature = Some(sig_cid);
                }
                Ok(None) => {}
                // The field waits for its signature; the other fields' go
                // out in the same wave.
                Err(BuildError::NeedsInput(request)) => {
                    first_miss.get_or_insert(request);
                    continue;
                }
                Err(error) => return Err(error),
            }
        }

        let field_block_bytes = field_block
            .to_dag_cbor()
            .map_err(|e| format!("Failed to encode field block: {}", e))?;
        let field_cid = generate_cid_from_bytes(&field_block_bytes)
            .map_err(|e| format!("Failed to generate field CID: {}", e))?;

        blockstore_entries.push((cid_bytes(&field_cid), field_block_bytes.into()));

        head_write.add(
            &crate::block::heads::document::name(field_name, &field_cid),
            priority,
        );
        headstore_entries.push((
            priority_index_key(identity.doc_short_id, priority, field_cid),
            vec![],
        ));

        field_links.push(DAGLink::new(field_name.clone(), field_cid));
        field_cids.push(field_cid);
    }

    // The composite links every field block, so it waits for all of them.
    if let Some(request) = first_miss {
        return Err(BuildError::NeedsInput(request));
    }

    if let Some(block) = immutable {
        if let Some((sig_cid, sig_cbor)) = block.signature.clone() {
            blockstore_entries.push((cid_bytes(&sig_cid), sig_cbor));
        }
        blockstore_entries.push((cid_bytes(&block.cid), block.bytes.clone()));
        block.add_heads(&mut head_write);
        headstore_entries.push((
            priority_index_key(identity.doc_short_id, priority, block.cid),
            vec![],
        ));
        field_links.push(block.link());
        field_cids.push(block.cid);
    }

    // Composite encryption CID for doc-level encryption
    let composite_encryption_cid = if let Some(enc) = encryption_config {
        if enc.encrypt_doc {
            let key = encryption_key_prepared(&doc_ref_bytes, None, prepared_inputs)?;
            let enc_block = Encryption { key: key.to_vec() };
            let enc_bytes = enc_block
                .to_dag_cbor()
                .map_err(|e| format!("Failed to encode composite encryption block: {}", e))?;
            let enc_cid = generate_cid_from_bytes(&enc_bytes)
                .map_err(|e| format!("Failed to generate composite encryption CID: {}", e))?;
            blockstore_entries.push((cid_bytes(&enc_cid), enc_bytes.into()));
            Some(enc_cid)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(enc_cid) = composite_encryption_cid {
        encryption_cids.push(enc_cid);
    }

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
        if let Some((sig_cid, sig_cbor)) =
            compute_signature_prepared(&composite_block, signer, prepared_inputs)?
        {
            blockstore_entries.push((cid_bytes(&sig_cid), sig_cbor));
            composite_block.signature = Some(sig_cid);
        }
    }

    let composite_bytes = composite_block
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode composite block: {}", e))?;
    let composite_cid = generate_cid_from_bytes(&composite_bytes)
        .map_err(|e| format!("Failed to generate composite CID: {}", e))?;

    let composite_bytes = Bytes::from(composite_bytes);
    blockstore_entries.push((cid_bytes(&composite_cid), composite_bytes.clone()));

    head_write.add(
        &crate::block::heads::document::name("C", &composite_cid),
        priority,
    );
    headstore_entries.push((
        priority_index_key(identity.doc_short_id, priority, composite_cid),
        vec![],
    ));

    Ok(ComputedBlocks {
        blockstore_entries,
        headstore_entries,
        head_write,
        doc_short_id: identity.doc_short_id,
        block_result: BlockResult {
            cid: composite_cid,
            block: composite_bytes,
            doc_id: derive_doc_id(&composite_cid),
            field_cids,
            encryption_cids,
        },
    })
}

/// Batch-insert pre-computed blocks into blockstore and headstore, their CIDs
/// collected into `batch_session` once they commit.
pub async fn insert_computed_blocks(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    blocks: &ComputedBlocks,
    batch_session: Option<&str>,
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
    crate::block::heads::document::commit(headstore, blocks.doc_short_id, &blocks.head_write)
        .await
        .map_err(|e| format!("Failed to write heads: {}", e))?;
    collect_on_commit(
        blockstore,
        batch_session,
        blocks
            .block_result
            .field_cids
            .iter()
            .copied()
            .chain([blocks.block_result.cid]),
    )
    .await?;
    Ok(())
}
