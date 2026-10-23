use super::*;
use crate::block::heads::document as doc_heads;

/// The `Encryption` link a new block should carry forward, read off the
/// previous block, when the write brings no config of its own.
///
/// Mirrors the fallback half of Go's `determineBlockEncryption`
/// (internal/core/block/store.go): the DAG, not any process-local state, is
/// what records that a document is encrypted, and the existing link is reused
/// so no new encryption block is written.
///
/// A head whose block cannot be read is an error rather than a miss: guessing
/// "not encrypted" is exactly the silent downgrade this derivation exists to
/// prevent. Matches Go, which returns `ErrCouldNotFindBlock` here.
///
/// Heads with no encryption are skipped rather than ending the search, and an
/// empty `heads` yields `None`. A field with nothing of its own to inherit is
/// not thereby in the clear: `write_document_blocks` falls back to the
/// document-level policy the composite head records before writing plaintext.
async fn inherited_encryption_cid(
    blockstore: &NamespaceView,
    heads: &[Cid],
) -> Result<Option<Cid>, String> {
    for head in heads {
        let block_bytes = blockstore
            .get(&cid_bytes(head))
            .await
            .map_err(|e| format!("Failed to read previous block {}: {}", head, e))?
            .ok_or_else(|| format!("Previous block {} not found", head))?;
        let prev = Block::from_dag_cbor(&block_bytes)
            .map_err(|e| format!("Failed to decode previous block {}: {}", head, e))?;

        if prev.encryption.is_some() {
            return Ok(prev.encryption);
        }
    }

    Ok(None)
}

/// Last resort for an `Encryption` block neither block namespace holds: ask the
/// configured KMS, which may keep it in a `KeyStore` of its own.
///
/// `DefraKms` serves what it holds locally before any peer fan-out
/// (`kms/src/defra_kms.rs`), so this stays a local read in the common case.
async fn kms_resolved_key(
    kms: Option<&std::sync::Arc<dyn kms::KmsService>>,
    enc_cid: Cid,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<Option<([u8; 32], Cid)>, BuildError> {
    let kms_svc = kms.ok_or_else(|| format!("Encryption block {} not found", enc_cid))?;
    if let Some(inputs) = prepared_inputs {
        let request = crate::write::prepare::kms_input_request(
            crate::write::prepare::PreparedKmsOperation::Get {
                cid: enc_cid.to_string(),
            },
        );
        let Some(response) = inputs.get(&request) else {
            return Err(BuildError::NeedsInput(request));
        };
        let response: crate::write::prepare::PreparedKmsResponse =
            serde_json::from_slice(&response).map_err(|error| {
                BuildError::Message(format!("invalid cached KMS key response: {error}"))
            })?;
        let returned_cid = Cid::try_from(response.cid.as_str()).map_err(|error| {
            BuildError::Message(format!("invalid CID in cached KMS key response: {error}"))
        })?;
        let key: [u8; 32] = response.key.try_into().map_err(|key: Vec<u8>| {
            BuildError::Message(format!(
                "KMS returned a {}-byte key for {returned_cid}, expected 32",
                key.len()
            ))
        })?;
        if returned_cid != enc_cid {
            return Err(BuildError::Message(format!(
                "KMS returned key for {returned_cid}, expected {enc_cid}"
            )));
        }
        return Ok(Some((key, enc_cid)));
    }
    let key = kms_svc
        .get_keys(&kms::RequestContext::anonymous(), &[enc_cid])
        .await
        .map_err(|e| format!("kms get_keys for {}: {}", enc_cid, e))?
        .wait_all()
        .await
        .map_err(|e| format!("kms get_keys for {}: {}", enc_cid, e))?
        .remove(&enc_cid)
        .ok_or_else(|| format!("Encryption block {} not found", enc_cid))?;

    Ok(Some((key, enc_cid)))
}

/// The inherited encryption link together with the key it points at, for the
/// field deltas that actually need to encrypt with it.
///
/// Reusing one key across a field's whole history is safe only because every
/// delta is sealed with a fresh random nonce (see `encrypt_delta`). Do not make
/// that nonce deterministic or counter-based without revisiting this.
async fn inherited_encryption(
    blockstore: &NamespaceView,
    heads: &[Cid],
    kms: Option<&std::sync::Arc<dyn kms::KmsService>>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<Option<([u8; 32], Cid)>, BuildError> {
    let Some(enc_cid) = inherited_encryption_cid(blockstore, heads).await? else {
        return Ok(None);
    };

    // Encstore first, then blockstore: blocks arriving over P2P land in the
    // encstore while locally-written ones go to the blockstore. Mirrors the
    // merge decrypt path (`db-merge/src/merge_handler/encryption.rs`) and
    // `versioned_fetcher`.
    let enc_key = cid_bytes(&enc_cid);
    let enc_bytes = match blockstore
        .sibling(storage::namespace::Namespace::Encstore)
        .get(&enc_key)
        .await
        .map_err(|e| format!("Failed to read encryption block {}: {}", enc_cid, e))?
    {
        Some(bytes) => bytes,
        None => match blockstore
            .get(&enc_key)
            .await
            .map_err(|e| format!("Failed to read encryption block {}: {}", enc_cid, e))?
        {
            Some(bytes) => bytes,
            // A `KmsService` only promises that `generate_key` persisted the
            // block in its own `KeyStore`; nothing requires that store to be
            // either of these namespaces. Ask it for the key rather than
            // assuming the aliasing today's wiring happens to give us.
            None => return kms_resolved_key(kms, enc_cid, prepared_inputs).await,
        },
    };
    let enc_block = Encryption::from_dag_cbor(&enc_bytes)
        .map_err(|e| format!("Failed to decode encryption block {}: {}", enc_cid, e))?;
    let key: [u8; 32] = enc_block.key.as_slice().try_into().map_err(|_| {
        format!(
            "Encryption block {} has a {}-byte key, expected 32",
            enc_cid,
            enc_block.key.len()
        )
    })?;

    Ok(Some((key, enc_cid)))
}

pub(crate) async fn inherited_encryption_policy(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    identity: DocRef,
    fields: &[String],
) -> Result<(bool, rapidhash::RapidHashSet<String>), String> {
    let snapshot = DocHeadsSnapshot::load(headstore, identity.doc_short_id).await?;
    let heads = snapshot.field_heads("C");
    let encrypt_doc = inherited_encryption_cid(blockstore, &heads)
        .await?
        .is_some();
    let mut encrypted_fields = rapidhash::RapidHashSet::default();
    for field in fields {
        let heads = snapshot.field_heads(field);
        if inherited_encryption_cid(blockstore, &heads)
            .await?
            .is_some()
        {
            encrypted_fields.insert(field.clone());
        }
    }
    Ok((encrypt_doc, encrypted_fields))
}

/// Mint a fresh key for a field, seal its delta with it, and persist the
/// `Encryption` block the new block will link to.
///
/// Shared by the explicit-config path and the document-policy path so both
/// route through the KMS whenever one is configured.
#[allow(clippy::too_many_arguments)]
async fn encrypt_with_new_key(
    blockstore: &NamespaceView,
    kms: Option<&std::sync::Arc<dyn kms::KmsService>>,
    doc_ref_bytes: &[u8],
    field_name: &str,
    key_field_name: Option<&str>,
    value_bytes: &[u8],
    prepared: Option<&document::WritePreparation>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<(Vec<u8>, Cid), BuildError> {
    if let Some(prepared) = prepared {
        if let Some((cid, key)) = prepared.key(field_name) {
            if prepared.key_scope_matches(field_name, key_field_name) {
                return Ok((encrypt_retry_delta(value_bytes, key, prepared_inputs)?, cid));
            }
        }
        if (prepared.requires_kms_keys() || kms.is_some()) && prepared_inputs.is_none() {
            return Err(format!(
                "encryption key for field {field_name} was not prepared for the current encryption context"
            )
            .into());
        }
    }
    if let Some(kms_svc) = kms {
        // KMS path: the KMS generates + persists the Encryption block (in its
        // KeyStore) and returns the CID + plain key for us to encrypt the field
        // delta with. The scope is keyed by the node-local DocRef: the public
        // DocID is not yet known on the create path.
        let doc_id = hex::encode(doc_ref_bytes);
        let field = key_field_name.map(str::to_owned);
        let (enc_cid, key) = if let Some(inputs) = prepared_inputs {
            let request = crate::write::prepare::kms_input_request(
                crate::write::prepare::PreparedKmsOperation::Generate { doc_id, field },
            );
            let Some(response) = inputs.get(&request) else {
                return Err(BuildError::NeedsInput(request));
            };
            let response: crate::write::prepare::PreparedKmsResponse =
                serde_json::from_slice(&response).map_err(|error| {
                    BuildError::Message(format!("invalid cached KMS key response: {error}"))
                })?;
            let enc_cid = Cid::try_from(response.cid.as_str()).map_err(|error| {
                BuildError::Message(format!("invalid CID in cached KMS key response: {error}"))
            })?;
            let key: [u8; 32] = response.key.try_into().map_err(|key: Vec<u8>| {
                BuildError::Message(format!(
                    "KMS returned a {}-byte key for {enc_cid}, expected 32",
                    key.len()
                ))
            })?;
            (enc_cid, key)
        } else {
            let scope = kms::KeyScope::Document { doc_id, field };
            let ctx = kms::RequestContext::anonymous();
            kms_svc
                .generate_key(&ctx, scope)
                .await
                .map_err(|e| BuildError::Message(format!("kms generate_key: {e}")))?
        };
        let encrypted = encrypt_retry_delta(value_bytes, &key, prepared_inputs)?;
        tracing::debug!(
            field = %field_name,
            ciphertext_len = encrypted.len(),
            "Encrypted field delta via KMS"
        );
        return Ok((encrypted, enc_cid));
    }

    // Legacy path: inline key generation + direct block store.
    let key = encryption_key_prepared(doc_ref_bytes, key_field_name, prepared_inputs)?;
    let encrypted = encrypt_retry_delta(value_bytes, &key, prepared_inputs)?;

    tracing::debug!(
        field = %field_name,
        ciphertext_len = encrypted.len(),
        "Encrypted field delta"
    );

    let enc_block = Encryption { key: key.to_vec() };
    let enc_bytes = enc_block
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode encryption block: {}", e))?;
    let enc_cid = generate_cid_from_bytes(&enc_bytes)
        .map_err(|e| format!("Failed to generate encryption CID: {}", e))?;
    blockstore
        .set(&cid_bytes(&enc_cid), &enc_bytes)
        .await
        .map_err(|e| format!("Failed to store encryption block: {}", e))?;

    Ok((encrypted, enc_cid))
}

/// Write document blocks to blockstore and heads to headstore.
///
/// This function creates CRDT blocks for a document and writes:
/// 1. LWW field blocks to blockstore
/// 2. Composite block to blockstore
/// 3. Head CIDs to headstore (for _commits queries)
///
/// This matches Go DefraDB's ProcessBlock -> updateHeads flow.
///
/// # Arguments
///
/// * `blockstore` - Transaction blockstore view
/// * `headstore` - Transaction headstore view
/// * `doc` - The document to build blocks from
/// * `collection_version_id` - The collection version ID for CRDT deltas
/// * `modified_fields` - Optional set of field names that were modified.
///   If Some, only these fields will have new blocks created.
///   If None (for create operations), all fields get new blocks.
///
/// # Returns
///
/// Returns `BlockResult` containing the composite block and metadata.
#[allow(clippy::too_many_arguments)]
pub async fn write_document_blocks(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc: &Document,
    schema: &CollectionVersion,
    identity: DocRef,
    modified_fields: Option<&rapidhash::RapidHashSet<String>>,
    encryption_config: Option<&EncryptionConfig>,
    signing_config: Option<&SigningConfig>,
    kms: Option<&std::sync::Arc<dyn kms::KmsService>>,
) -> Result<BlockResult, String> {
    write_document_blocks_with_inputs(
        blockstore,
        headstore,
        doc,
        schema,
        identity,
        modified_fields,
        encryption_config,
        signing_config,
        None,
        kms,
        None,
        None,
    )
    .await
    .map_err(|error| error.to_string())
}

/// [`write_document_blocks`] with prepared remote inputs. An update that has
/// already read the document's heads passes them in `heads`, so they are not
/// read a second time; without them an update reads them here.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_document_blocks_with_inputs(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc: &Document,
    schema: &CollectionVersion,
    identity: DocRef,
    modified_fields: Option<&rapidhash::RapidHashSet<String>>,
    encryption_config: Option<&EncryptionConfig>,
    signing_config: Option<&SigningConfig>,
    batch_session: Option<&str>,
    kms: Option<&std::sync::Arc<dyn kms::KmsService>>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
    heads: Option<DocHeadsSnapshot>,
) -> Result<BlockResult, BuildError> {
    let collection_version_id = schema.version_id.as_str();
    let immutable_fields = immutable_field_names(schema);
    let doc_ref_bytes = identity.encode();

    let mut field_links: Vec<DAGLink> = Vec::new();
    let mut field_cids: Vec<Cid> = Vec::new();
    let mut initial_field_cids: Vec<Cid> = Vec::new();
    let mut encryption_cids: Vec<Cid> = Vec::new();

    let is_create = modified_fields.is_none();
    let snapshot = match heads {
        _ if is_create => None,
        Some(heads) => Some(heads),
        None => Some(DocHeadsSnapshot::load(headstore, identity.doc_short_id).await?),
    };
    let mut head_write = storage::merge::head_set::Write::default();
    let composite_priority: u64 = if is_create {
        1
    } else {
        snapshot
            .as_ref()
            .ok_or_else(|| "snapshot required for updates".to_string())?
            .max_priority()
            + 1
    };

    let composite_heads: Vec<Cid> = if is_create {
        Vec::new()
    } else {
        snapshot
            .as_ref()
            .ok_or_else(|| "snapshot required for updates".to_string())?
            .field_heads("C")
    };

    // The composite block's encryption link is set only for whole-document
    // encryption, so its presence is what records that policy in the DAG. Read
    // once, before the field loop, and reused for the composite block below.
    // Skipped when this write sets the policy itself, since every field then
    // takes the explicit path anyway.
    let explicit_doc_encryption = encryption_config.is_some_and(|enc| enc.encrypt_doc);
    let inherited_doc_encryption_cid = if explicit_doc_encryption {
        None
    } else {
        inherited_encryption_cid(blockstore, &composite_heads).await?
    };

    // The first signature this pass is missing; the pass goes on past it.
    let mut first_miss = None;
    let (immutable_names, immutable) = if is_create {
        immutable_for_pass(
            doc,
            schema,
            encryption_config,
            signing_config,
            prepared_inputs,
            &mut first_miss,
        )?
    } else {
        Default::default()
    };
    for (field_name, field_value) in doc.values() {
        if field_name == "_docID" {
            continue;
        }

        // On update an immutable field is unchanged (validated before blocks
        // are written), so it needs no block; on create it is in the
        // Immutable block.
        let should_create_block = match modified_fields {
            None => !immutable_names.contains(field_name),
            Some(fields) => {
                fields.contains(field_name) && !immutable_fields.contains(field_name.as_str())
            }
        };

        let field_head_entries = if is_create {
            Vec::new()
        } else {
            snapshot
                .as_ref()
                .ok_or_else(|| "snapshot required for updates".to_string())?
                .field_heads(field_name)
        };

        if should_create_block {
            let heads = field_head_entries.clone();
            let field_priority = if is_create {
                1
            } else {
                snapshot
                    .as_ref()
                    .ok_or_else(|| "snapshot required for updates".to_string())?
                    .field_max_priority(field_name)
                    + 1
            };

            let cbor_value = if let Some(delta) = doc.get_counter_delta(field_name) {
                delta
            } else {
                field_value.value()
            };

            let value_bytes = encode_value_as_cbor(cbor_value)?;

            let explicit = encryption_config.filter(|enc| enc.should_encrypt_field(field_name));
            let (value_bytes, encryption_cid) = if let Some(enc) = explicit {
                let key_field_name = if enc.should_encrypt_individual_field(field_name) {
                    Some(field_name.as_str())
                } else {
                    None
                };

                let (encrypted, enc_cid) = encrypt_with_new_key(
                    blockstore,
                    kms,
                    &doc_ref_bytes,
                    field_name,
                    key_field_name,
                    &value_bytes,
                    doc.write_preparation().map(AsRef::as_ref),
                    prepared_inputs,
                )
                .await?;
                encryption_cids.push(enc_cid);
                (encrypted, Some(enc_cid))
            } else if let Some((key, enc_cid)) =
                inherited_encryption(blockstore, &heads, kms, prepared_inputs).await?
            {
                let encrypted = encrypt_retry_delta(&value_bytes, &key, prepared_inputs)?;

                tracing::debug!(
                    field = %field_name,
                    ciphertext_len = encrypted.len(),
                    "Encrypted field delta with the previous block's key"
                );

                (encrypted, Some(enc_cid))
            } else if inherited_doc_encryption_cid.is_some() {
                // The document is encrypted as a whole but this field has no
                // history of its own to inherit from, so mint it a key under
                // the document-level policy rather than dropping to plaintext.
                let (encrypted, enc_cid) = encrypt_with_new_key(
                    blockstore,
                    kms,
                    &doc_ref_bytes,
                    field_name,
                    None,
                    &value_bytes,
                    doc.write_preparation().map(AsRef::as_ref),
                    prepared_inputs,
                )
                .await?;
                encryption_cids.push(enc_cid);
                (encrypted, Some(enc_cid))
            } else {
                (value_bytes, None)
            };

            let is_counter = doc
                .fields()
                .get(field_name)
                .map(|f| f.crdt_type().is_counter())
                .unwrap_or(false)
                || doc.get_counter_delta(field_name).is_some();

            let nonce: i64 = if is_counter && !is_create {
                if let Some(inputs) = prepared_inputs {
                    let exact = serde_json::to_vec(&(
                        "counter-nonce-v1",
                        &doc_ref_bytes,
                        field_name,
                        field_priority,
                        &heads,
                        &value_bytes,
                    ))
                    .map_err(|error| BuildError::Message(error.to_string()))?;
                    let request = query::prepared::PreparedInputRequest::new(
                        query::prepared::PreparedInputKind::Encryption,
                        exact,
                    );
                    let nonce = match inputs.get(&request) {
                        Some(nonce) => nonce,
                        None => {
                            let nonce: i64 = rand::random();
                            inputs
                                .insert(request, std::sync::Arc::<[u8]>::from(nonce.to_be_bytes()))
                        }
                    };
                    i64::from_be_bytes(nonce.as_ref().try_into().map_err(|_| {
                        BuildError::Message("invalid prepared counter nonce".into())
                    })?)
                } else {
                    rand::random()
                }
            } else {
                0
            };

            let delta = if is_counter {
                CrdtDelta::Counter(CounterDeltaPayload {
                    field_name: field_name.clone(),
                    priority: field_priority,
                    nonce,
                    collection_version_id: collection_version_id.to_string(),
                    data: value_bytes,
                })
            } else {
                CrdtDelta::Lww(LwwDeltaPayload {
                    field_name: field_name.clone(),
                    priority: field_priority,
                    collection_version_id: collection_version_id.to_string(),
                    data: value_bytes,
                })
            };

            let mut field_block =
                Block::new_with_options(delta, heads.clone(), vec![], encryption_cid, None);

            if let Some(signer) = signing_config {
                tracing::debug!(field = %field_name, priority = field_priority, "Signing field block");
                match sign_block(&field_block, signer, blockstore, prepared_inputs).await {
                    Ok(Some(sig_cid)) => {
                        tracing::debug!(field = %field_name, sig_cid = %sig_cid, "Field block signed");
                        field_block.signature = Some(sig_cid);
                    }
                    Ok(None) => {
                        tracing::debug!(field = %field_name, priority = field_priority, "Skipped signing (priority > 1)");
                    }
                    // The field waits for its signature; the other fields'
                    // go out in the same wave.
                    Err(BuildError::NeedsInput(request)) => {
                        first_miss.get_or_insert(request);
                        continue;
                    }
                    Err(e) => {
                        tracing::debug!(field = %field_name, error = %e, "Failed to sign field block");
                        return Err(e);
                    }
                }
            } else {
                tracing::debug!(field = %field_name, "No signing config for field");
            }

            let field_block_bytes = field_block
                .to_dag_cbor()
                .map_err(|e| format!("Failed to encode field block: {}", e))?;
            let field_cid = generate_cid_from_bytes(&field_block_bytes)
                .map_err(|e| format!("Failed to generate field CID: {}", e))?;

            tracing::debug!(
                field = %field_name,
                cid = %field_cid,
                bytes_len = field_block_bytes.len(),
                signature = ?field_block.signature,
                "Storing field block"
            );
            if tracing::enabled!(tracing::Level::DEBUG) {
                match Block::from_dag_cbor(&field_block_bytes) {
                    Ok(b) => tracing::debug!(signature = ?b.signature, "Signature roundtrip OK"),
                    Err(e) => tracing::debug!(error = %e, "Signature roundtrip failed"),
                }
            }

            blockstore
                .set(&cid_bytes(&field_cid), &field_block_bytes)
                .await
                .map_err(|e| format!("Failed to store field block: {}", e))?;

            for old_head in &field_head_entries {
                head_write.supersede(&doc_heads::name(field_name, old_head));
            }
            head_write.add(&doc_heads::name(field_name, &field_cid), field_priority);

            headstore
                .set(
                    &priority_index_key(identity.doc_short_id, field_priority, field_cid),
                    &[],
                )
                .await
                .map_err(|e| format!("Failed to write field priority index: {}", e))?;

            tracing::debug!(
                field_name = %field_name,
                cid = %field_cid,
                is_counter = is_counter,
                has_prev_head = !heads.is_empty(),
                "Stored field block and head"
            );

            field_links.push(DAGLink::new(field_name.clone(), field_cid));
            field_cids.push(field_cid);
            if field_priority == 1 {
                initial_field_cids.push(field_cid);
            }
        }
    }
    // The composite links every field block, so it waits for all of them.
    if let Some(request) = first_miss {
        return Err(BuildError::NeedsInput(request));
    }

    if let Some(block) = immutable {
        if let Some((sig_cid, sig_cbor)) = &block.signature {
            blockstore
                .set(&cid_bytes(sig_cid), sig_cbor)
                .await
                .map_err(|e| format!("Failed to store signature block: {}", e))?;
        }
        blockstore
            .set(&cid_bytes(&block.cid), &block.bytes)
            .await
            .map_err(|e| format!("Failed to store immutable block: {}", e))?;
        block.add_heads(&mut head_write);
        headstore
            .set(
                &priority_index_key(identity.doc_short_id, 1, block.cid),
                &[],
            )
            .await
            .map_err(|e| format!("Failed to write immutable priority index: {}", e))?;
        field_links.push(block.link());
        field_cids.push(block.cid);
        initial_field_cids.push(block.cid);
    }

    let composite_payload = CompositeDeltaPayload {
        collection_version_id: collection_version_id.to_string(),
        priority: composite_priority,
        status: 1,
    };

    let composite_encryption_cid = if explicit_doc_encryption {
        let key = encryption_key_prepared(&doc_ref_bytes, None, prepared_inputs)?;
        let enc_block = Encryption { key: key.to_vec() };
        let enc_bytes = enc_block
            .to_dag_cbor()
            .map_err(|e| format!("Failed to encode composite encryption block: {}", e))?;
        let enc_cid = generate_cid_from_bytes(&enc_bytes)
            .map_err(|e| format!("Failed to generate composite encryption CID: {}", e))?;
        blockstore
            .set(&cid_bytes(&enc_cid), &enc_bytes)
            .await
            .map_err(|e| format!("Failed to store composite encryption block: {}", e))?;
        encryption_cids.push(enc_cid);
        Some(enc_cid)
    } else {
        // The composite payload is never encrypted with the key, only linked,
        // so the CID alone is enough here.
        inherited_doc_encryption_cid
    };

    let mut composite_block = Block::new_with_options(
        CrdtDelta::Composite(composite_payload),
        composite_heads.clone(),
        field_links,
        composite_encryption_cid,
        None,
    );

    if let Some(signer) = signing_config {
        if let Some(sig_cid) =
            sign_block(&composite_block, signer, blockstore, prepared_inputs).await?
        {
            composite_block.signature = Some(sig_cid);
        }
    }

    let composite_bytes = composite_block
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode composite block: {}", e))?;
    let composite_cid = generate_cid_from_bytes(&composite_bytes)
        .map_err(|e| format!("Failed to generate composite CID: {}", e))?;

    blockstore
        .set(&cid_bytes(&composite_cid), &composite_bytes)
        .await
        .map_err(|e| format!("Failed to store composite block: {}", e))?;

    for old_head in &composite_heads {
        head_write.supersede(&doc_heads::name("C", old_head));
    }
    head_write.add(&doc_heads::name("C", &composite_cid), composite_priority);

    headstore
        .set(
            &priority_index_key(identity.doc_short_id, composite_priority, composite_cid),
            &[],
        )
        .await
        .map_err(|e| format!("Failed to write composite priority index: {}", e))?;
    doc_heads::commit(headstore, identity.doc_short_id, &head_write)
        .await
        .map_err(|e| format!("Failed to write document heads: {}", e))?;

    let doc_id_str = if is_create {
        derive_doc_id(&composite_cid)
    } else {
        doc.id()
            .ok_or_else(|| "Document must have an ID for updates".to_string())?
            .to_string()
    };

    tracing::debug!(
        doc_id = %doc_id_str,
        cid = %composite_cid,
        field_count = field_cids.len(),
        prev_heads = composite_heads.len(),
        "Built composite block with field links and wrote heads"
    );

    collect_on_commit(
        blockstore,
        batch_session,
        initial_field_cids.into_iter().chain([composite_cid]),
    )
    .await?;

    Ok(BlockResult {
        cid: composite_cid,
        block: composite_bytes.into(),
        doc_id: doc_id_str,
        field_cids,
        encryption_cids,
    })
}

/// Write a delete block (composite with status=2) to blockstore and heads to headstore.
pub async fn write_delete_block(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc_id: &str,
    doc_short_id: u64,
    collection_version_id: &str,
    signing_config: Option<&SigningConfig>,
) -> Result<BlockResult, String> {
    write_delete_block_with_inputs(
        blockstore,
        headstore,
        doc_id,
        doc_short_id,
        collection_version_id,
        signing_config,
        None,
        None,
        None,
    )
    .await
    .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_delete_block_with_inputs(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc_id: &str,
    doc_short_id: u64,
    collection_version_id: &str,
    signing_config: Option<&SigningConfig>,
    batch_session: Option<&str>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
    heads: Option<DocHeadsSnapshot>,
) -> Result<BlockResult, BuildError> {
    let snapshot = match heads {
        Some(heads) => heads,
        None => DocHeadsSnapshot::load(headstore, doc_short_id).await?,
    };
    let mut head_write = storage::merge::head_set::Write::default();
    let priority: u64 = snapshot.max_priority() + 1;

    let composite_heads = snapshot.field_heads("C");

    let composite_payload = CompositeDeltaPayload {
        collection_version_id: collection_version_id.to_string(),
        priority,
        status: 2,
    };

    let composite_encryption_cid = inherited_encryption_cid(blockstore, &composite_heads).await?;

    let mut composite_block = Block::new_with_options(
        CrdtDelta::Composite(composite_payload),
        composite_heads.clone(),
        vec![],
        composite_encryption_cid,
        None,
    );

    if let Some(signer) = signing_config {
        if let Some(sig_cid) =
            sign_block(&composite_block, signer, blockstore, prepared_inputs).await?
        {
            composite_block.signature = Some(sig_cid);
        }
    }

    let composite_bytes = composite_block
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode delete composite block: {}", e))?;
    let composite_cid = generate_cid_from_bytes(&composite_bytes)
        .map_err(|e| format!("Failed to generate delete composite CID: {}", e))?;

    blockstore
        .set(&cid_bytes(&composite_cid), &composite_bytes)
        .await
        .map_err(|e| format!("Failed to store delete composite block: {}", e))?;

    for old_head in &composite_heads {
        head_write.supersede(&doc_heads::name("C", old_head));
    }
    head_write.add(&doc_heads::name("C", &composite_cid), priority);

    headstore
        .set(
            &priority_index_key(doc_short_id, priority, composite_cid),
            &[],
        )
        .await
        .map_err(|e| format!("Failed to write delete priority index: {}", e))?;
    doc_heads::commit(headstore, doc_short_id, &head_write)
        .await
        .map_err(|e| format!("Failed to write delete heads: {}", e))?;

    tracing::debug!(
        doc_id = %doc_id,
        cid = %composite_cid,
        priority = priority,
        "Built delete composite block (status=2)"
    );

    collect_on_commit(blockstore, batch_session, [composite_cid]).await?;

    Ok(BlockResult {
        cid: composite_cid,
        block: composite_bytes.into(),
        doc_id: doc_id.to_string(),
        field_cids: vec![],
        encryption_cids: vec![],
    })
}

fn encrypt_retry_delta(
    plaintext: &[u8],
    key: &[u8; 32],
    inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<Vec<u8>, String> {
    match inputs {
        Some(inputs) => encrypt_delta_prepared(plaintext, key, inputs),
        None => encrypt_delta(plaintext, key),
    }
}
