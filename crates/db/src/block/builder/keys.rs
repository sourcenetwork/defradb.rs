use super::*;
use std::sync::Arc;

/// An encryption key resolved before any block is computed.
///
/// `block` holds the `Encryption` block to store when the key was minted
/// inline; a KMS persists its own and leaves it `None`.
#[derive(Debug, Clone)]
pub struct ResolvedKey {
    pub key: [u8; 32],
    pub cid: Cid,
    pub block: Option<Bytes>,
}

/// The keys a document create encrypts with: one per encrypted field, plus the
/// composite's link under whole-document encryption.
#[derive(Debug, Clone, Default)]
pub struct DocumentKeys {
    pub fields: RapidHashMap<String, ResolvedKey>,
    pub composite: Option<ResolvedKey>,
}

/// Resolve every key a create of `doc` needs, so block computation itself
/// stays pure. Key generation through a KMS is async and persists the key in
/// the KMS's own transaction, which is why it cannot happen mid-computation.
pub async fn resolve_document_keys(
    doc: &Document,
    identity: DocStorageIdentity,
    encryption_config: Option<&EncryptionConfig>,
    kms: Option<&Arc<dyn kms::KmsService>>,
) -> Result<DocumentKeys, String> {
    let mut keys = DocumentKeys::default();
    let Some(enc) = encryption_config else {
        return Ok(keys);
    };
    let doc_ref_bytes = identity.doc_ref_bytes();

    for field_name in doc.values().keys() {
        if field_name == "_docID" || !enc.should_encrypt_field(field_name) {
            continue;
        }
        let key_field_name = enc
            .should_encrypt_individual_field(field_name)
            .then_some(field_name.as_str());
        let key = new_key(kms, &doc_ref_bytes, key_field_name).await?;
        keys.fields.insert(field_name.clone(), key);
    }

    if enc.encrypt_doc {
        keys.composite = Some(inline_key(&doc_ref_bytes, None)?);
    }
    Ok(keys)
}

/// Mint a fresh key, through the KMS when one is configured.
pub(super) async fn new_key(
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
        cid,
        block: None,
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
        cid,
        block: Some(bytes.into()),
    })
}
