use super::*;
use std::sync::Arc;

/// An `Encryption` block a new block links to.
///
/// `block` holds the bytes to store when this write minted the key inline; a
/// KMS persists its own, and an inherited key is already stored.
#[derive(Debug, Clone)]
pub struct KeyLink {
    pub cid: Cid,
    pub block: Option<Bytes>,
    /// Whether this write created the key, so its block is new and owned by
    /// the document.
    pub minted: bool,
}

/// A field's encryption key together with the block that records it.
#[derive(Debug, Clone)]
pub struct ResolvedKey {
    pub key: [u8; 32],
    pub link: KeyLink,
}

/// Where a new block sits in its DAG.
#[derive(Debug, Clone, Default)]
pub struct DagPosition {
    pub heads: Vec<Cid>,
    pub priority: u64,
    /// Head keys this block supersedes.
    pub stale_heads: Vec<Vec<u8>>,
}

/// Everything about one field block that depends on storage, a KMS or chance.
#[derive(Debug, Clone)]
pub struct FieldPlan {
    pub position: DagPosition,
    pub key: Option<ResolvedKey>,
    pub nonce: i64,
}

/// Every input to block computation that needs storage, a KMS or randomness,
/// resolved up front so [`compute_document_blocks`](super::compute_document_blocks)
/// stays a pure function. A create is the plan with no heads.
#[derive(Debug, Clone)]
pub struct BlockPlan {
    pub fields: RapidHashMap<String, FieldPlan>,
    pub composite: DagPosition,
    pub composite_key: Option<KeyLink>,
    /// A create derives its DocID from the genesis composite; an update keeps
    /// the document's.
    pub is_create: bool,
}

/// Plan the blocks for writing `doc`. `modified_fields` is `None` for a create,
/// which gets a block for every field and no heads; an update gets blocks for
/// the modified fields on top of the document's current heads.
#[allow(clippy::too_many_arguments)]
pub async fn plan_document_blocks(
    blockstore: &NamespaceView,
    headstore: &NamespaceView,
    doc: &Document,
    identity: DocStorageIdentity,
    modified_fields: Option<&rapidhash::RapidHashSet<String>>,
    encryption_config: Option<&EncryptionConfig>,
    kms: Option<&Arc<dyn kms::KmsService>>,
) -> Result<BlockPlan, String> {
    let doc_ref_bytes = identity.doc_ref_bytes();
    let snapshot = match modified_fields {
        None => None,
        Some(_) => Some(DocHeadsSnapshot::load(headstore, identity.doc_short_id).await?),
    };
    let position = |field: &str| match &snapshot {
        None => DagPosition {
            priority: 1,
            ..DagPosition::default()
        },
        Some(snapshot) => {
            let entries = snapshot.field_heads(field);
            let priority = if field == "C" {
                snapshot.max_priority()
            } else {
                snapshot.field_max_priority(field)
            } + 1;
            DagPosition {
                heads: entries.iter().map(|h| h.cid).collect(),
                priority,
                stale_heads: entries.into_iter().map(|h| h.key).collect(),
            }
        }
    };

    let composite = position("C");
    // The composite block's encryption link is set only for whole-document
    // encryption, so its presence is what records that policy in the DAG.
    let explicit_doc_encryption = encryption_config.is_some_and(|enc| enc.encrypt_doc);
    let inherited_doc_encryption = if explicit_doc_encryption {
        None
    } else {
        inherited_encryption_cid(blockstore, &composite.heads).await?
    };

    let mut fields = RapidHashMap::new();
    for field_name in doc.values().keys() {
        if field_name == "_docID" || modified_fields.is_some_and(|m| !m.contains(field_name)) {
            continue;
        }
        let position = position(field_name);
        let explicit = encryption_config.filter(|enc| enc.should_encrypt_field(field_name));
        let key = if let Some(enc) = explicit {
            let key_field_name = enc
                .should_encrypt_individual_field(field_name)
                .then_some(field_name.as_str());
            Some(new_key(kms, &doc_ref_bytes, key_field_name).await?)
        } else if let Some(key) = inherited_encryption(blockstore, &position.heads, kms).await? {
            Some(key)
        } else if inherited_doc_encryption.is_some() {
            // The document is encrypted as a whole but this field has no
            // history of its own to inherit from, so mint it a key under the
            // document-level policy rather than dropping to plaintext.
            Some(new_key(kms, &doc_ref_bytes, None).await?)
        } else {
            None
        };
        // A counter delta on top of history needs a nonce so two equal
        // increments stay distinct blocks.
        let nonce = if snapshot.is_some() {
            rand::random()
        } else {
            0
        };
        fields.insert(
            field_name.clone(),
            FieldPlan {
                position,
                key,
                nonce,
            },
        );
    }

    let composite_key = if explicit_doc_encryption {
        Some(inline_key(&doc_ref_bytes, None)?.link)
    } else {
        inherited_doc_encryption.map(|cid| KeyLink {
            cid,
            block: None,
            minted: false,
        })
    };

    Ok(BlockPlan {
        fields,
        composite,
        composite_key,
        is_create: modified_fields.is_none(),
    })
}

/// Mint a fresh key, through the KMS when one is configured.
async fn new_key(
    kms: Option<&Arc<dyn kms::KmsService>>,
    doc_ref_bytes: &[u8],
    key_field_name: Option<&str>,
) -> Result<ResolvedKey, String> {
    let Some(kms_svc) = kms else {
        return inline_key(doc_ref_bytes, key_field_name);
    };
    // Scoped by the node-local DocRef: the public DocID does not exist until
    // the genesis composite is computed.
    let scope = kms::KeyScope::Document {
        doc_id: hex::encode(doc_ref_bytes),
        field: key_field_name.map(str::to_owned),
    };
    let (cid, key) = kms_svc
        .generate_key(&kms::RequestContext::anonymous(), scope)
        .await
        .map_err(|e| format!("kms generate_key: {e}"))?;
    Ok(ResolvedKey {
        key,
        link: KeyLink {
            cid,
            block: None,
            minted: true,
        },
    })
}

fn inline_key(doc_ref_bytes: &[u8], key_field_name: Option<&str>) -> Result<ResolvedKey, String> {
    let key = defra_core::encryption::generate_encryption_key_for(doc_ref_bytes, key_field_name);
    let bytes = Encryption { key: key.to_vec() }
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode encryption block: {}", e))?;
    let cid = generate_cid_from_bytes(&bytes)
        .map_err(|e| format!("Failed to generate encryption CID: {}", e))?;
    Ok(ResolvedKey {
        key,
        link: KeyLink {
            cid,
            block: Some(bytes.into()),
            minted: true,
        },
    })
}

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
/// not thereby in the clear: the plan falls back to the document-level policy
/// the composite head records before writing plaintext.
pub(super) async fn inherited_encryption_cid(
    blockstore: &NamespaceView,
    heads: &[Cid],
) -> Result<Option<Cid>, String> {
    for head in heads {
        let block_bytes = blockstore
            .get(&head.to_bytes())
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
    kms: Option<&Arc<dyn kms::KmsService>>,
    enc_cid: Cid,
) -> Result<[u8; 32], String> {
    let kms_svc = kms.ok_or_else(|| format!("Encryption block {} not found", enc_cid))?;
    kms_svc
        .get_keys(&kms::RequestContext::anonymous(), &[enc_cid])
        .await
        .map_err(|e| format!("kms get_keys for {}: {}", enc_cid, e))?
        .wait_all()
        .await
        .map_err(|e| format!("kms get_keys for {}: {}", enc_cid, e))?
        .remove(&enc_cid)
        .ok_or_else(|| format!("Encryption block {} not found", enc_cid))
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
    kms: Option<&Arc<dyn kms::KmsService>>,
) -> Result<Option<ResolvedKey>, String> {
    let Some(cid) = inherited_encryption_cid(blockstore, heads).await? else {
        return Ok(None);
    };
    let link = KeyLink {
        cid,
        block: None,
        minted: false,
    };

    // Encstore first, then blockstore: blocks arriving over P2P land in the
    // encstore while locally-written ones go to the blockstore. Mirrors the
    // merge decrypt path (`merge/merge_handler/encryption.rs`) and
    // `versioned_fetcher`.
    let enc_key = cid.to_bytes();
    let read_error = |e| format!("Failed to read encryption block {}: {}", cid, e);
    let enc_bytes = match blockstore
        .sibling(storage::namespace::Namespace::Encstore)
        .get(&enc_key)
        .await
        .map_err(read_error)?
    {
        Some(bytes) => bytes,
        None => match blockstore.get(&enc_key).await.map_err(read_error)? {
            Some(bytes) => bytes,
            // A `KmsService` only promises that `generate_key` persisted the
            // block in its own `KeyStore`; nothing requires that store to be
            // either of these namespaces. Ask it for the key rather than
            // assuming the aliasing today's wiring happens to give us.
            None => {
                let key = kms_resolved_key(kms, cid).await?;
                return Ok(Some(ResolvedKey { key, link }));
            }
        },
    };
    let enc_block = Encryption::from_dag_cbor(&enc_bytes)
        .map_err(|e| format!("Failed to decode encryption block {}: {}", cid, e))?;
    let key: [u8; 32] = enc_block.key.as_slice().try_into().map_err(|_| {
        format!(
            "Encryption block {} has a {}-byte key, expected 32",
            cid,
            enc_block.key.len()
        )
    })?;

    Ok(Some(ResolvedKey { key, link }))
}
