//! An operation-based field's merge: each block's operation applies to the
//! document value exactly once. The applied key is the one-winner race that
//! makes a redelivered block a no-op.
use super::*;
use crdt::stored::StoredDocument;
use crdt::FieldCrdt;
use defra_core::block::FieldDelta;

use super::field::FieldMergeResult;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Apply a block's operation to `field` once.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn apply_field_op_once(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        crdt: &dyn FieldCrdt,
        collection: &Collection,
        field: &FieldDescription,
        delta: &FieldDelta<'_>,
        doc_short_id: u64,
    ) -> std::result::Result<FieldMergeResult, MergeError> {
        let failed = |error: crdt::FieldCrdtError| MergeError::MergeFailed(error.to_string());
        let storage = |error: storage::corekv::Error| MergeError::Storage(error.to_string());
        let input = crdt
            .decode(field, delta.data)
            .map_err(|error| MergeError::BlockDecode(format!("field '{}': {error}", field.name)))?;
        let op = crdt.op(input, false).map_err(failed)?;
        let applied_key =
            storage::keys::HeadstorePriorityKey::new(doc_short_id, delta.priority, *cid).bytes();
        let applied = !headstore.has(&applied_key).await.map_err(storage)?;

        let value = match op_target(datastore, collection, doc_short_id, field).await? {
            Some(part) => {
                let doc_key = collection.doc_key(doc_short_id);
                if applied {
                    let state = crdt
                        .block_state(field, delta.priority, delta.data)
                        .map_err(failed)?;
                    let operand =
                        storage::merge::document_part(part, state.part()).ok_or_else(|| {
                            MergeError::MergeFailed(format!(
                                "invalid operation on '{}'",
                                field.name
                            ))
                        })?;
                    datastore.merge(&doc_key, &operand).await.map_err(storage)?;
                }
                // The read is never validated: no decision rests on it.
                datastore
                    .get_parts(&doc_key, &[])
                    .await
                    .map_err(storage)?
                    .as_deref()
                    .and_then(StoredDocument::decode)
                    .and_then(|stored| stored.fields.get(&part)?.materialize())
            }
            None => {
                // A value on another schema version (or predating part IDs) is
                // rewritten whole by its composite, so the operation applies
                // to the stored value here and the composite puts the result.
                let stored = collection
                    .read_document_unvalidated(datastore, doc_short_id)
                    .await
                    .map_err(MergeError::Database)?;
                let stored = stored.as_ref().and_then(|doc| doc.get(&field.name));
                if applied {
                    Some(crdt.apply(field, stored, &op).map_err(failed)?)
                } else {
                    stored.cloned()
                }
            }
        };
        if applied {
            headstore
                .set(&applied_key, crate::block::heads::document::APPLIED)
                .await
                .map_err(storage)?;
        }
        Ok(FieldMergeResult {
            applied,
            value,
            order: None,
        })
    }

    pub(crate) async fn best_effort_finalize_linked_field_blocks(&self, cids: &[Cid]) {
        if cids.is_empty() {
            return;
        }
        // The blockstore's merged-set is the single source of truth for CRDT
        // idempotency (matching Go): every ingest path (PushLog, DAG traversal,
        // crash-recovery replay) skips blocks it holds. See #847.
        if let Err(error) = self.blockstore.mark_batch_as_merged(cids).await {
            tracing::warn!(
                count = cids.len(),
                error = %error,
                "Failed to finalize merged linked field blocks"
            );
        }
    }
}

/// The field's part of the document value, as an operand target, when the
/// stored value is on `collection`'s schema version and has part IDs. `None`
/// otherwise: such a value is rewritten whole by its composite.
async fn op_target(
    datastore: &NamespaceView,
    collection: &Collection,
    doc_short_id: u64,
    field: &FieldDescription,
) -> std::result::Result<Option<u32>, MergeError> {
    let Some(part) = collection
        .part_ids(datastore)
        .await
        .map_err(MergeError::Database)?
        .and_then(|ids| ids.get(&field.name).copied())
    else {
        return Ok(None);
    };
    let version = collection
        .load_version(datastore, doc_short_id)
        .await
        .map_err(MergeError::Database)?;
    Ok((version.as_deref() == Some(collection.version_id())).then_some(part))
}
