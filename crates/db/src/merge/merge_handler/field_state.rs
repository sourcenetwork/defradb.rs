//! A state-based field's merge: the incoming state joins the field's register,
//! the datastore entry merges resolve against.
use super::*;
use crate::collection::value::FieldOrder;
use crdt::stored::StoredField;
use crdt::FieldCrdt;
use defra_core::block::FieldDelta;
use storage::keys::CRDTValueKey;

use super::field::FieldMergeResult;

/// The register of one field of one document: its state's bytes and its
/// priority, under two datastore keys. Both reads are validated, since the
/// merge decides on them.
struct Register {
    value_key: Vec<u8>,
    priority_key: Vec<u8>,
}

impl Register {
    fn new(collection_version_id: &str, doc_id: &str, field_name: &str) -> Self {
        let value_key = CRDTValueKey::new(
            collection_version_id.to_owned(),
            doc_id.as_bytes().to_vec(),
            field_name.to_owned(),
        );
        Self {
            priority_key: value_key.priority_key().bytes(),
            value_key: value_key.bytes(),
        }
    }

    /// The register's state; a register never written holds its type's
    /// starting state at priority 0.
    async fn read(
        &self,
        datastore: &NamespaceView,
        start: StoredField,
    ) -> std::result::Result<StoredField, MergeError> {
        let storage = |error: storage::corekv::Error| MergeError::Storage(error.to_string());
        let priority = datastore
            .get(&self.priority_key)
            .await
            .map_err(storage)?
            .map_or(0, |bytes| {
                crate::block::builder::decode_priority_varint(&bytes)
            });
        let value = datastore
            .get(&self.value_key)
            .await
            .map_err(storage)?
            .map(|bytes| bytes.to_vec())
            .unwrap_or_default();
        Ok(StoredField {
            priority,
            value,
            ..start
        })
    }

    /// Store `state`; empty bytes remove the value key.
    async fn write(
        &self,
        datastore: &NamespaceView,
        state: &StoredField,
    ) -> std::result::Result<(), MergeError> {
        let storage = |error: storage::corekv::Error| MergeError::Storage(error.to_string());
        if state.value.is_empty() {
            datastore.delete(&self.value_key).await.map_err(storage)?;
        } else {
            datastore
                .set(&self.value_key, &state.value)
                .await
                .map_err(storage)?;
        }
        datastore
            .set(
                &self.priority_key,
                &crate::block::builder::encode_priority_varint(state.priority),
            )
            .await
            .map_err(storage)
    }
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// Join the state of the document value into `state` when the headstore
    /// is ahead of the register.
    ///
    /// Two priority stores can disagree for a field: the headstore tracks the
    /// authoritative current priority (advanced by BOTH local writes and
    /// merges), while the register is advanced ONLY by merges. A local write
    /// therefore pushes the headstore (and materialized doc) ahead of a stale
    /// register. When the headstore is ahead, the document's value at the
    /// headstore priority joins the register, so the merge resolves against
    /// the true current state instead of the stale entry (otherwise a
    /// re-walked ancestor delta clobbers a concurrent local write and replicas
    /// diverge). Returns whether the document exists, and whether `state`
    /// changed.
    #[allow(clippy::too_many_arguments)]
    async fn reseed_from_document(
        &self,
        headstore: &NamespaceView,
        datastore: &NamespaceView,
        crdt: &dyn FieldCrdt,
        field: &FieldDescription,
        collection: Option<&Collection>,
        state: &mut StoredField,
        doc_id_str: &str,
        doc_short_id: u64,
        reads: &mut super::block_reads::BlockReads,
    ) -> std::result::Result<(bool, bool), MergeError> {
        let Some(collection) = collection else {
            return Ok((false, false));
        };
        let hs_priority = reads
            .field_priority(headstore, doc_short_id, &field.name)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?;
        // Fast path: the register is strictly ahead of the headstore, so
        // nothing in the materialized doc can improve it. Equal priority
        // still needs the merge below to apply the tie-break.
        if state.priority != 0 && state.priority > hs_priority {
            return Ok((true, false));
        }
        let Ok(doc_id) = DocID::from_string(doc_id_str) else {
            return Ok((false, false));
        };
        let Some(existing_doc) = reads
            .document(collection, datastore, doc_short_id, &doc_id)
            .await?
        else {
            return Ok((false, false));
        };
        let Some(field_value) = existing_doc.get(&field.name) else {
            return Ok((true, false));
        };
        if hs_priority == 0 {
            return Ok((true, false));
        }
        let failed = |error: crdt::FieldCrdtError| MergeError::MergeFailed(error.to_string());
        let seed = crdt
            .block_state(
                field,
                hs_priority,
                &crdt.encode(field_value).map_err(failed)?,
            )
            .map_err(failed)?;
        let seeded = crdt
            .merge(state, seed)
            .ok_or_else(|| MergeError::MergeFailed("malformed register state".into()))?
            .was_applied();
        Ok((true, seeded))
    }

    /// Join a block's state into the field's register and report the winner.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn merge_field_state(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        crdt: &dyn FieldCrdt,
        field: &FieldDescription,
        delta: &FieldDelta<'_>,
        collection: Option<&Collection>,
        doc_id_str: &str,
        doc_short_id: u64,
        reads: &mut super::block_reads::BlockReads,
    ) -> std::result::Result<FieldMergeResult, MergeError> {
        let failed = |error: crdt::FieldCrdtError| MergeError::MergeFailed(error.to_string());
        let incoming = crdt
            .block_state(field, delta.priority, delta.data)
            .map_err(failed)?;
        let start = StoredField {
            name: incoming.name.clone(),
            kind: incoming.kind,
            priority: 0,
            value: Vec::new(),
        };
        let register = Register::new(delta.collection_version_id, doc_id_str, &field.name);
        let mut state = register.read(datastore, start.clone()).await?;
        let (doc_exists, seeded) = self
            .reseed_from_document(
                headstore,
                datastore,
                crdt,
                field,
                collection,
                &mut state,
                doc_id_str,
                doc_short_id,
                reads,
            )
            .await?;
        // A document's first block resolves against nothing: the register
        // holds no state of a document that does not exist yet.
        if delta.priority == 1 && !doc_exists {
            state = start;
        }
        let applied = crdt
            .merge(&mut state, incoming)
            .ok_or_else(|| MergeError::MergeFailed("malformed register state".into()))?
            .was_applied();
        if applied || seeded {
            register.write(datastore, &state).await?;
        }
        let value = crdt.materialize(state.part()).ok_or_else(|| {
            MergeError::BlockDecode(format!("undecodable state of field {}", field.name))
        })?;
        Ok(FieldMergeResult {
            applied,
            value: Some(value),
            order: Some(FieldOrder {
                priority: state.priority,
                raw_value: Some(state.value),
            }),
            deferred: false,
            op: None,
            parked: None,
        })
    }
}
