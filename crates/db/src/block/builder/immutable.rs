use std::collections::BTreeMap;

use super::*;
use crate::block::heads::document as doc_heads;
use defra_core::block::{ImmutableDeltaPayload, IMMUTABLE_LINK_NAME};

/// A document's Immutable block, computed at create.
pub(crate) struct ImmutableBlock {
    pub cid: Cid,
    pub bytes: Bytes,
    pub signature: Option<(Cid, Bytes)>,
    /// Fields the block carries; they get no LWW block of their own.
    pub fields: rapidhash::RapidHashSet<String>,
}

impl ImmutableBlock {
    pub fn link(&self) -> DAGLink {
        DAGLink::new(IMMUTABLE_LINK_NAME, self.cid)
    }

    /// The block is the head of every field it carries, at priority 1, so a
    /// peer holding one of them as mutable chains its update to it. The
    /// block's priority index entry is the caller's to write.
    pub fn add_heads(&self, write: &mut storage::merge::head_set::Write) {
        for field in &self.fields {
            write.add(&doc_heads::name(field, &self.cid), 1);
        }
    }
}

/// The names of the fields `schema` declares `@immutable`.
pub(crate) fn immutable_field_names(schema: &CollectionVersion) -> rapidhash::RapidHashSet<&str> {
    schema
        .fields
        .iter()
        .filter(|field| field.immutable)
        .map(|field| field.name.as_str())
        .collect()
}

/// The fields the Immutable block of a document being created carries, CBOR
/// encoded: every `@immutable` field the encryption config leaves in
/// plaintext, since a block carries one encryption link, so an encrypted
/// field keeps its own block and key. Empty when no field qualifies.
pub(crate) fn immutable_fields(
    doc: &Document,
    schema: &CollectionVersion,
    encryption_config: Option<&EncryptionConfig>,
) -> Result<BTreeMap<String, serde_bytes::ByteBuf>, String> {
    let immutable = immutable_field_names(schema);
    let mut fields = BTreeMap::new();
    if immutable.is_empty() {
        return Ok(fields);
    }
    for (field_name, field_value) in doc.values() {
        if field_name == "_docID"
            || !immutable.contains(field_name.as_str())
            || encryption_config.is_some_and(|enc| enc.should_encrypt_field(field_name))
        {
            continue;
        }
        let data = encode_value_as_cbor(field_value.value())?;
        fields.insert(field_name.clone(), serde_bytes::ByteBuf::from(data));
    }
    Ok(fields)
}

/// Build the Immutable block over `fields`, as [`immutable_fields`] gave
/// them. A remote signature this request has not resolved yet is asked for
/// with [`BuildError::NeedsInput`].
pub(crate) fn immutable_block(
    fields: BTreeMap<String, serde_bytes::ByteBuf>,
    schema: &CollectionVersion,
    signing_config: Option<&SigningConfig>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<ImmutableBlock, BuildError> {
    let names = fields.keys().cloned().collect();
    let delta = CrdtDelta::Immutable(ImmutableDeltaPayload {
        priority: 1,
        collection_version_id: schema.version_id.clone(),
        fields,
    });
    let mut block = Block::new(delta, vec![], vec![]);
    let signature = match signing_config {
        Some(signer) => compute_signature_prepared(&block, signer, prepared_inputs)?,
        None => None,
    };
    block.signature = signature.as_ref().map(|(cid, _)| *cid);

    let bytes = block
        .to_dag_cbor()
        .map_err(|e| format!("Failed to encode immutable block: {}", e))?;
    let cid = generate_cid_from_bytes(&bytes)
        .map_err(|e| format!("Failed to generate immutable block CID: {}", e))?;
    Ok(ImmutableBlock {
        cid,
        bytes: bytes.into(),
        signature,
        fields: names,
    })
}

/// Build the Immutable block for a document being created; `None` when no
/// field qualifies.
pub(crate) fn compute_immutable_block(
    doc: &Document,
    schema: &CollectionVersion,
    encryption_config: Option<&EncryptionConfig>,
    signing_config: Option<&SigningConfig>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
) -> Result<Option<ImmutableBlock>, BuildError> {
    let fields = immutable_fields(doc, schema, encryption_config)?;
    if fields.is_empty() {
        return Ok(None);
    }
    immutable_block(fields, schema, signing_config, prepared_inputs).map(Some)
}

/// The Immutable block of a create pass, built alongside its field blocks:
/// the names of the fields it carries, which get no LWW block, and the block,
/// `None` when no field qualifies or its signature is still an input this
/// pass asked for in `first_miss`.
pub(crate) fn immutable_for_pass(
    doc: &Document,
    schema: &CollectionVersion,
    encryption_config: Option<&EncryptionConfig>,
    signing_config: Option<&SigningConfig>,
    prepared_inputs: Option<&query::prepared::PreparedInputMemo>,
    first_miss: &mut Option<query::prepared::PreparedInputRequest>,
) -> Result<(rapidhash::RapidHashSet<String>, Option<ImmutableBlock>), BuildError> {
    let fields = immutable_fields(doc, schema, encryption_config)?;
    let names = fields.keys().cloned().collect();
    if fields.is_empty() {
        return Ok((names, None));
    }
    match immutable_block(fields, schema, signing_config, prepared_inputs) {
        Ok(block) => Ok((names, Some(block))),
        // The block waits for its signature; the field blocks' go out in
        // the same wave.
        Err(BuildError::NeedsInput(request)) => {
            first_miss.get_or_insert(request);
            Ok((names, None))
        }
        Err(error) => Err(error),
    }
}
