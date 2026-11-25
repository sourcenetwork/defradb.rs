use super::*;
use crate::block::heads::document as doc_heads;

/// The `Encryption` link of a field's first encrypted head, which the field's
/// next block reuses when the write brings no config for it.
///
/// This is Go's `determineBlockEncryption` without a config
/// (internal/core/block/store.go): the DAG, not any process-local state,
/// records that a field is encrypted, and the existing link is reused so no
/// new encryption block is written. Heads with no encryption are skipped
/// rather than ending the search, and an empty `heads` yields `None`.
///
/// A head whose block cannot be read is an error rather than a miss: guessing
/// "not encrypted" is exactly the silent downgrade this derivation exists to
/// prevent. Matches Go, which returns `ErrCouldNotFindBlock` here.
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

/// Where the next block of a field takes its encryption from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldEncryptionSource {
    /// The write's own config names the field: a fresh key, scoped to the
    /// field itself when the config lists it individually.
    Explicit { individual: bool },
    /// The `Encryption` link of the field's first encrypted head, reused.
    Inherit(Cid),
    /// A field with no heads on a document whose every field is encrypted:
    /// a fresh document-scope key.
    DocScopeNew,
    /// No config, and nothing encrypts the field.
    Plaintext,
}

/// Whether a field with no heads gets a fresh document-scope key: decided
/// once per write, either by a KMS preparation that read the heads the write
/// builds on, or lazily from the document's heads the first time a field
/// with no heads asks.
pub struct DocScope<'a> {
    snapshot: Option<&'a DocHeadsSnapshot>,
    prepared: Option<&'a rapidhash::RapidHashSet<String>>,
    pending: Option<&'a rapidhash::RapidHashMap<String, bool>>,
    every_field_encrypted: Option<bool>,
}

impl<'a> DocScope<'a> {
    /// A write's document scope. A KMS preparation's decision wins over the
    /// snapshot, so the write never needs a document key that was not
    /// prepared for it. Pass a preparation only when it read the heads this
    /// write builds on: one taken before the attempt can be stale, and the
    /// builder then decides on its own snapshot (`build_document_blocks`).
    pub fn new(
        snapshot: Option<&'a DocHeadsSnapshot>,
        preparation: Option<&'a document::WritePreparation>,
    ) -> Self {
        Self {
            snapshot,
            prepared: preparation
                .filter(|prepared| prepared.requires_kms_keys())
                .map(|prepared| &prepared.doc_scope_fields),
            pending: None,
            every_field_encrypted: None,
        }
    }

    /// The scope a KMS preparation decides, from the committed heads plus the
    /// fields earlier writes of the same request prepared but have not
    /// committed yet, each with whether it is encrypted.
    fn preparing(
        snapshot: &'a DocHeadsSnapshot,
        pending: &'a rapidhash::RapidHashMap<String, bool>,
    ) -> Self {
        Self {
            snapshot: Some(snapshot),
            prepared: None,
            pending: Some(pending),
            every_field_encrypted: None,
        }
    }

    async fn covers(&mut self, blockstore: &NamespaceView, field: &str) -> Result<bool, String> {
        if let Some(prepared) = self.prepared {
            return Ok(prepared.contains(field));
        }
        if let Some(known) = self.every_field_encrypted {
            return Ok(known);
        }
        let known = every_field_encrypted(blockstore, self.snapshot, self.pending).await?;
        self.every_field_encrypted = Some(known);
        Ok(known)
    }
}

/// Whether the document has at least one field and every field is encrypted:
/// a pending field by what its write prepared, any other by its heads. Stops
/// at the first field that is not.
async fn every_field_encrypted(
    blockstore: &NamespaceView,
    snapshot: Option<&DocHeadsSnapshot>,
    pending: Option<&rapidhash::RapidHashMap<String, bool>>,
) -> Result<bool, String> {
    let mut any = false;
    if let Some(pending) = pending {
        if pending.values().any(|encrypted| !encrypted) {
            return Ok(false);
        }
        any = !pending.is_empty();
    }
    for (field, heads) in snapshot.into_iter().flat_map(DocHeadsSnapshot::fields) {
        if pending.is_some_and(|pending| pending.contains_key(field)) {
            continue;
        }
        if inherited_encryption_cid(blockstore, heads).await?.is_none() {
            return Ok(false);
        }
        any = true;
    }
    Ok(any)
}

/// The one encryption decision for a field's next block, shared by the block
/// builder and the KMS key preparation so the two cannot disagree:
/// - an explicit config for the field wins;
/// - a field with heads follows them, as Go's `determineBlockEncryption`
///   does: the first encrypted head's link, or plaintext when none is;
/// - a field with no heads gets a fresh document-scope key only when the
///   document has a field and every field is encrypted, and is plaintext
///   otherwise. Go writes it in plaintext; this is a deliberate divergence
///   (`defra_core::encryption`).
pub async fn field_encryption_source(
    blockstore: &NamespaceView,
    heads: &[Cid],
    field: &str,
    config: Option<&EncryptionConfig>,
    doc_scope: &mut DocScope<'_>,
) -> Result<FieldEncryptionSource, String> {
    if let Some(config) = config.filter(|config| config.should_encrypt_field(field)) {
        return Ok(FieldEncryptionSource::Explicit {
            individual: config.should_encrypt_individual_field(field),
        });
    }
    if !heads.is_empty() {
        return Ok(match inherited_encryption_cid(blockstore, heads).await? {
            Some(enc_cid) => FieldEncryptionSource::Inherit(enc_cid),
            None => FieldEncryptionSource::Plaintext,
        });
    }
    Ok(if doc_scope.covers(blockstore, field).await? {
        FieldEncryptionSource::DocScopeNew
    } else {
        FieldEncryptionSource::Plaintext
    })
}

/// Whether a document's next block of `field` is encrypted when the write
/// carries no config of its own, by [`field_encryption_source`] over the
/// document's current heads.
pub(crate) async fn encrypts_field(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    identity: DocRef,
    field: &str,
) -> Result<bool, String> {
    let snapshot = DocHeadsSnapshot::load(headstore, identity.doc_short_id).await?;
    let heads = snapshot.field_heads(field);
    let mut doc_scope = DocScope::new(Some(&snapshot), None);
    let source = field_encryption_source(blockstore, &heads, field, None, &mut doc_scope).await?;
    Ok(source != FieldEncryptionSource::Plaintext)
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
) -> Result<([u8; 32], Cid), BuildError> {
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
        return Ok((key, enc_cid));
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

    Ok((key, enc_cid))
}

/// The key an inherited encryption link points at, for the field delta that
/// encrypts with it.
///
/// Reusing one key across a field's whole history is safe only because every
/// delta is sealed with a fresh random nonce (see `encrypt_delta`). Do not make
/// that nonce deterministic or counter-based without revisiting this.
async fn inherited_encryption(
    blockstore: &NamespaceView,
    enc_cid: Cid,
    kms: Option<&std::sync::Arc<dyn kms::KmsService>>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<([u8; 32], Cid), BuildError> {
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

    Ok((key, enc_cid))
}

/// What a KMS preparation decides for an update's fields, before the write:
/// the fields whose source is [`FieldEncryptionSource::DocScopeNew`], for it
/// to mint their document key, and every field with whether the write leaves
/// it encrypted, for a later write of the same request to decide on.
///
/// `pending` holds the fields earlier writes of the same request prepared but
/// have not committed: they count as fields with heads, encrypted as their
/// write prepared them.
pub(crate) async fn doc_scope_fields(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    identity: DocRef,
    fields: &[String],
    config: Option<&EncryptionConfig>,
    pending: &rapidhash::RapidHashMap<String, bool>,
) -> Result<
    (
        rapidhash::RapidHashSet<String>,
        rapidhash::RapidHashMap<String, bool>,
    ),
    String,
> {
    let snapshot = DocHeadsSnapshot::load(headstore, identity.doc_short_id).await?;
    let mut doc_scope = DocScope::preparing(&snapshot, pending);
    let mut scoped = rapidhash::RapidHashSet::default();
    let mut written = rapidhash::RapidHashMap::default();
    for field in fields {
        let explicit = config.is_some_and(|config| config.should_encrypt_field(field));
        let encrypted = match pending.get(field) {
            Some(encrypted) => explicit || *encrypted,
            None => {
                let heads = snapshot.field_heads(field);
                let source =
                    field_encryption_source(blockstore, &heads, field, config, &mut doc_scope)
                        .await?;
                if source == FieldEncryptionSource::DocScopeNew {
                    scoped.insert(field.clone());
                }
                source != FieldEncryptionSource::Plaintext
            }
        };
        written.insert(field.clone(), encrypted);
    }
    Ok((scoped, written))
}

/// Mint a fresh key for a field, seal its delta with it, and queue the
/// `Encryption` block the new block will link to in `blockstore_entries`.
///
/// Shared by the explicit-config path and the document-policy path so both
/// route through the KMS whenever one is configured.
#[allow(clippy::too_many_arguments)]
async fn encrypt_with_new_key(
    blockstore_entries: &mut Vec<(Vec<u8>, Bytes)>,
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
            let Some(response) = inputs.get_or_want(&request) else {
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
    blockstore_entries.push((cid_bytes(&enc_cid), enc_bytes.into()));

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

/// [`write_document_blocks`] with prepared remote inputs: every block is
/// built first, then all of them are stored. An update that has already read
/// the document's heads passes them in `heads`, so they are not read a second
/// time; without them an update reads them here.
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
    let built = build_document_blocks(
        blockstore,
        headstore,
        doc,
        schema,
        identity,
        modified_fields,
        encryption_config,
        signing_config,
        kms,
        prepared_inputs,
        heads,
    )
    .await?;
    insert_computed_blocks(blockstore, headstore, &built, batch_session).await?;
    Ok(built.block_result)
}

/// Every block a write of `doc` adds, built without storing anything. It
/// reads what the blocks depend on (the document's heads, the encryption its
/// previous blocks carry) and none of the blocks it builds, so storing them
/// afterwards, all at once, writes what storing each as it was built did.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn build_document_blocks(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc: &Document,
    schema: &CollectionVersion,
    identity: DocRef,
    modified_fields: Option<&rapidhash::RapidHashSet<String>>,
    encryption_config: Option<&EncryptionConfig>,
    signing_config: Option<&SigningConfig>,
    kms: Option<&std::sync::Arc<dyn kms::KmsService>>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
    heads: Option<DocHeadsSnapshot>,
) -> Result<ComputedBlocks, BuildError> {
    let mut blockstore_entries: Vec<(Vec<u8>, Bytes)> = Vec::new();
    let mut headstore_entries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
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

    // The composite carries no encryption link, as Go's (store.go `addDelta`
    // encrypts only field deltas), so a field's encryption is decided from
    // the field heads alone.
    //
    // With a memo the preparation ran before this attempt, so its headless
    // decision can be stale: a merge that reached an earlier write's field
    // in between changes what that write builds (rule (b)) but not what the
    // later write prepared. The attempt decides on its own heads instead. A
    // document key it needs that the preparation did not mint is a KMS input
    // the memo lacks: the attempt ends, the key is prepared at the edge, and
    // the next attempt decides again (`encrypt_with_new_key`).
    let mut doc_scope = DocScope::new(
        snapshot.as_ref(),
        doc.write_preparation()
            .map(AsRef::as_ref)
            .filter(|_| prepared_inputs.is_none()),
    );

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

            let crdt = field_crdt_of(blockstore.crdts(), doc, field_name)?;
            if is_create {
                crdt.op(field_value.value().clone(), true)
                    .map_err(|error| error.to_string())?;
            }
            let Some(value_bytes) = crdt
                .block_data(field_value.value(), doc.pending_op(field_name))
                .map_err(|error| error.to_string())?
            else {
                continue;
            };

            let source = field_encryption_source(
                blockstore,
                &heads,
                field_name,
                encryption_config,
                &mut doc_scope,
            )
            .await?;
            let fresh_key_field = match source {
                FieldEncryptionSource::Explicit { individual: true } => {
                    Some(Some(field_name.as_str()))
                }
                FieldEncryptionSource::Explicit { individual: false }
                | FieldEncryptionSource::DocScopeNew => Some(None),
                FieldEncryptionSource::Inherit(_) | FieldEncryptionSource::Plaintext => None,
            };
            let (value_bytes, encryption_cid) = if let Some(key_field_name) = fresh_key_field {
                let (encrypted, enc_cid) = match encrypt_with_new_key(
                    &mut blockstore_entries,
                    kms,
                    &doc_ref_bytes,
                    field_name,
                    key_field_name,
                    &value_bytes,
                    doc.write_preparation().map(AsRef::as_ref),
                    prepared_inputs,
                )
                .await
                {
                    Ok(sealed) => sealed,
                    Err(BuildError::NeedsInput(request)) => {
                        first_miss.get_or_insert(request);
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                encryption_cids.push(enc_cid);
                (encrypted, Some(enc_cid))
            } else if let FieldEncryptionSource::Inherit(enc_cid) = source {
                let (key, enc_cid) =
                    inherited_encryption(blockstore, enc_cid, kms, prepared_inputs).await?;
                let encrypted = encrypt_retry_delta(&value_bytes, &key, prepared_inputs)?;

                tracing::debug!(
                    field = %field_name,
                    ciphertext_len = encrypted.len(),
                    "Encrypted field delta with the previous block's key"
                );

                (encrypted, Some(enc_cid))
            } else {
                (value_bytes, None)
            };

            let nonce = if crdt.replicates_ops() && !is_create {
                fresh_nonce(
                    prepared_inputs,
                    (
                        "counter-nonce-v1",
                        &doc_ref_bytes,
                        field_name,
                        field_priority,
                        &heads,
                        &value_bytes,
                    ),
                )?
            } else {
                0
            };

            let delta = crdt.delta(crdt::DeltaParts {
                field_name,
                priority: field_priority,
                collection_version_id,
                nonce,
                data: value_bytes,
            });

            let mut field_block =
                Block::new_with_options(delta, heads.clone(), vec![], encryption_cid, None);

            if let Some(signer) = signing_config {
                tracing::debug!(field = %field_name, priority = field_priority, "Signing field block");
                match compute_signature_prepared(&field_block, signer, prepared_inputs) {
                    Ok(Some((sig_cid, sig_cbor))) => {
                        blockstore_entries.push((cid_bytes(&sig_cid), sig_cbor));
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
                "Built field block"
            );
            if tracing::enabled!(tracing::Level::DEBUG) {
                match Block::from_dag_cbor(&field_block_bytes) {
                    Ok(b) => tracing::debug!(signature = ?b.signature, "Signature roundtrip OK"),
                    Err(e) => tracing::debug!(error = %e, "Signature roundtrip failed"),
                }
            }

            blockstore_entries.push((cid_bytes(&field_cid), field_block_bytes.into()));

            for old_head in &field_head_entries {
                head_write.supersede(&doc_heads::name(field_name, old_head));
            }
            head_write.add(&doc_heads::name(field_name, &field_cid), field_priority);

            headstore_entries.push((
                priority_index_key(identity.doc_short_id, field_priority, field_cid),
                vec![],
            ));

            tracing::debug!(
                field_name = %field_name,
                cid = %field_cid,
                crdt = %crdt.ctype(),
                has_prev_head = !heads.is_empty(),
                "Built field block and head"
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
        if let Some((sig_cid, sig_cbor)) = block.signature.clone() {
            blockstore_entries.push((cid_bytes(&sig_cid), sig_cbor));
        }
        blockstore_entries.push((cid_bytes(&block.cid), block.bytes.clone()));
        block.add_heads(&mut head_write);
        headstore_entries.push((
            priority_index_key(identity.doc_short_id, 1, block.cid),
            vec![],
        ));
        field_links.push(block.link());
        field_cids.push(block.cid);
        initial_field_cids.push(block.cid);
    }

    let composite_payload = CompositeDeltaPayload {
        collection_version_id: collection_version_id.to_string(),
        priority: composite_priority,
        status: 1,
    };

    // No encryption link, as Go's composite: the docID derives from this CID.
    let mut composite_block = Block::new_with_options(
        CrdtDelta::Composite(composite_payload),
        composite_heads.clone(),
        field_links,
        None,
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

    let composite_bytes: Bytes = composite_block
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode composite block: {}", e))?
        .into();
    let composite_cid = generate_cid_from_bytes(&composite_bytes)
        .map_err(|e| format!("Failed to generate composite CID: {}", e))?;

    blockstore_entries.push((cid_bytes(&composite_cid), composite_bytes.clone()));

    for old_head in &composite_heads {
        head_write.supersede(&doc_heads::name("C", old_head));
    }
    head_write.add(&doc_heads::name("C", &composite_cid), composite_priority);

    headstore_entries.push((
        priority_index_key(identity.doc_short_id, composite_priority, composite_cid),
        vec![],
    ));

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
        "Built composite block with field links"
    );

    let mut batch_signed = initial_field_cids;
    batch_signed.push(composite_cid);
    Ok(ComputedBlocks {
        blockstore_entries,
        headstore_entries,
        head_write,
        doc_short_id: identity.doc_short_id,
        block_result: BlockResult {
            cid: composite_cid,
            block: composite_bytes,
            doc_id: doc_id_str,
            field_cids,
            encryption_cids,
        },
        batch_signed,
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

    // A tombstone links no encryption, as Go's: only field deltas are encrypted.
    let mut composite_block = Block::new_with_options(
        CrdtDelta::Composite(composite_payload),
        composite_heads.clone(),
        vec![],
        None,
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

/// A fresh nonce for an update of an operation-based field, so equal
/// operations stay distinct blocks. Drawn once per exact block input and
/// memoized, so an attempt that runs again writes the same block.
pub(crate) fn fresh_nonce<I>(
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
    block_input: I,
) -> Result<i64, BuildError>
where
    I: serde::Serialize,
{
    let Some(inputs) = prepared_inputs else {
        return Ok(rand::random());
    };
    let exact =
        serde_json::to_vec(&block_input).map_err(|error| BuildError::Message(error.to_string()))?;
    let request = query::prepared::PreparedInputRequest::new(
        query::prepared::PreparedInputKind::Encryption,
        exact,
    );
    let nonce = match inputs.get(&request) {
        Some(nonce) => nonce,
        None => {
            let nonce: i64 = rand::random();
            inputs.insert(request, std::sync::Arc::<[u8]>::from(nonce.to_be_bytes()))
        }
    };
    nonce
        .as_ref()
        .try_into()
        .map(i64::from_be_bytes)
        .map_err(|_| BuildError::Message("invalid prepared counter nonce".into()))
}
