//! A causal field's merge ([`crdt::CausalCrdt`]): a block's operation counts
//! in its parent version, so it applies to the stored value only when its
//! parents are the field's heads, and otherwise the field's history replays.
//! The replay walks the whole history, not just back to a common ancestor: a
//! sequence places an element next to deleted ones, which the stored value no
//! longer holds.
use super::*;
use crate::collection::value::FieldOrder;
use crdt::{CausalBlock, CausalCrdt, FieldCrdt};
use defra_core::block::FieldDelta;

use super::field::FieldMergeResult;

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// Merge the block `cid` of the causal `field` into its stored value, and
    /// write the result as the field's part of the document value. A block
    /// already among the field's heads changes nothing; a block merged before
    /// but no longer a head is in the history already, so its replay is the
    /// stored value again. `parents` are the block's, read from the
    /// blockstore when the caller has not decoded the block.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn merge_field_causal(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        crdt: &dyn FieldCrdt,
        causal: &dyn CausalCrdt,
        collection: &Collection,
        field: &FieldDescription,
        delta: &FieldDelta<'_>,
        doc_id_str: &str,
        doc_short_id: u64,
        parents: Option<&[Cid]>,
        metadata: Option<&BlockMetadata<'_>>,
        reads: &mut super::block_reads::BlockReads,
    ) -> std::result::Result<FieldMergeResult, MergeError> {
        let failed = |error: crdt::FieldCrdtError| MergeError::MergeFailed(error.to_string());
        let storage = |error: String| MergeError::Storage(error);
        let doc_id = DocID::from_string(doc_id_str)
            .map_err(|e| MergeError::MergeFailed(format!("invalid doc_id: {e}")))?;
        // The stored value decides the result, so this read is validated: a
        // concurrent writer of the field aborts the merge, which runs again.
        let current = reads
            .document(collection, datastore, doc_short_id, &doc_id)
            .await?
            .and_then(|doc| doc.get(&field.name).cloned());
        let field_priority = reads
            .field_priority(headstore, doc_short_id, &field.name)
            .await
            .map_err(|e| storage(e.to_string()))?;
        let mut heads: Vec<Cid> = reads
            .heads(headstore, doc_short_id)
            .await
            .map_err(|e| storage(e.to_string()))?
            .live()
            .filter_map(|(name, _)| crate::block::heads::document::parse_name(name))
            .filter(|(name, _)| *name == field.name)
            .map(|(_, head)| head)
            .collect();
        heads.sort_unstable();

        let applied = !heads.contains(cid);
        let value = if applied {
            let mut parents = match parents {
                Some(parents) => parents.to_vec(),
                None => self.stored_block(cid).await?.heads.unwrap_or_default(),
            };
            parents.sort_unstable();
            if parents == heads {
                let op = causal.block_op(field, delta.data).map_err(failed)?;
                crdt.apply(field, current.as_ref(), &op).map_err(failed)?
            } else {
                heads.push(*cid);
                let history = self
                    .causal_history(&heads, cid, causal, field, delta.data, metadata)
                    .await?;
                causal.replay(field, &history).map_err(failed)?
            }
        } else {
            current.unwrap_or(NormalValue::Null)
        };
        let priority = field_priority.max(delta.priority);
        // A value the part merge does not write is written by the composite,
        // which writes the fields it is given an order for.
        let order = if applied
            && !self
                .merge_causal_part(
                    datastore,
                    crdt,
                    collection,
                    field,
                    priority,
                    &value,
                    doc_short_id,
                )
                .await?
        {
            Some(FieldOrder {
                priority,
                raw_value: None,
            })
        } else {
            None
        };
        Ok(FieldMergeResult {
            applied,
            value: Some(value),
            order,
            deferred: false,
        })
    }

    /// Write `value` as the field's part of the document value, when the
    /// stored value is on `collection`'s schema version, and say whether it
    /// was written. Any other value is rewritten whole by the block's
    /// composite.
    #[allow(clippy::too_many_arguments)]
    async fn merge_causal_part(
        &self,
        datastore: &NamespaceView,
        crdt: &dyn FieldCrdt,
        collection: &Collection,
        field: &FieldDescription,
        priority: u64,
        value: &NormalValue,
        doc_short_id: u64,
    ) -> std::result::Result<bool, MergeError> {
        let storage = |error: storage::corekv::Error| MergeError::Storage(error.to_string());
        let Some(part) =
            super::field_ops::op_target(datastore, collection, doc_short_id, field).await?
        else {
            return Ok(false);
        };
        let state = crdt
            .state(field, priority, value)
            .map_err(|error| MergeError::MergeFailed(error.to_string()))?;
        let operand = storage::merge::document_part(part, state.part()).ok_or_else(|| {
            MergeError::MergeFailed(format!("invalid merged value of '{}'", field.name))
        })?;
        datastore
            .merge(&collection.doc_key(doc_short_id), &operand)
            .await
            .map_err(storage)?;
        Ok(true)
    }

    async fn stored_block(&self, cid: &Cid) -> std::result::Result<Block, MergeError> {
        let bytes = self
            .blockstore
            .get(cid)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?
            .ok_or_else(|| MergeError::Storage(format!("block {cid} not found in blockstore")))?;
        Block::from_dag_cbor(&bytes).map_err(|e| MergeError::BlockDecode(e.to_string()))
    }

    /// Every block of a causal field's history under `tips`, each with the
    /// operation it makes. The block `cid` being merged carries `data`,
    /// already decrypted; the others are read from the blockstore, which holds
    /// them since a block merges only after its parents are local.
    async fn causal_history(
        &self,
        tips: &[Cid],
        cid: &Cid,
        causal: &dyn CausalCrdt,
        field: &FieldDescription,
        data: &[u8],
        metadata: Option<&BlockMetadata<'_>>,
    ) -> std::result::Result<Vec<CausalBlock>, MergeError> {
        let failed = |error: crdt::FieldCrdtError| MergeError::BlockDecode(error.to_string());
        let mut seen: RapidHashSet<Cid> = tips.iter().copied().collect();
        let mut stack = tips.to_vec();
        let mut history = Vec::new();
        while let Some(next) = stack.pop() {
            let block = self.stored_block(&next).await?;
            let parents = block.heads.clone().unwrap_or_default();
            let op = if next == *cid {
                causal.block_op(field, data).map_err(failed)?
            } else {
                let writer = crdt::delta_crdt(&block.delta, field.crdt_type)
                    .map_err(|error| MergeError::UnsupportedDelta(error.to_string()))?;
                let stored = block
                    .delta
                    .field()
                    .ok_or_else(|| MergeError::UnsupportedDelta("not a field delta".into()))?;
                let data = self
                    .decrypt_block_data(stored.data, block.encryption.as_ref(), metadata)
                    .await?;
                crdt::history_op(writer, field, &data).map_err(failed)?
            };
            for parent in &parents {
                if seen.insert(*parent) {
                    stack.push(*parent);
                }
            }
            history.push(CausalBlock {
                id: next.to_bytes(),
                parents: parents.iter().map(Cid::to_bytes).collect(),
                op,
            });
        }
        Ok(history)
    }
}
