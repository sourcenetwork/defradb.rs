//! A causal field's merge ([`crdt::CausalCrdt`]): a block's operation counts
//! in its parent version, so it applies to the stored value only when its
//! parents are the field's heads. Any other block is concurrent with one
//! already merged: the merge walks down from the heads and the block to the
//! field's last critical block (one every other block comes before or after)
//! and merges only the blocks above it ([`CausalCrdt::merge_from`]). When the
//! walk reaches a root first, or the type cannot merge from there, the whole
//! history replays, not just back to a common ancestor: a sequence places an
//! element next to deleted ones, which the stored value no longer holds.
use std::collections::BTreeSet;

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
            let value = if parents == heads {
                let op = causal.block_op(field, delta.data).map_err(failed)?;
                crdt.apply(field, current.as_ref(), &op).map_err(failed)?
            } else {
                let mut tips = heads.clone();
                tips.push(*cid);
                self.concurrent_value(
                    current.as_ref(),
                    &tips,
                    cid,
                    causal,
                    field,
                    delta.data,
                    metadata,
                )
                .await?
            };
            // With every merged block among its parents, the result is the
            // block's own version, which what the block records has to fit.
            if heads.iter().all(|head| parents.binary_search(head).is_ok()) {
                causal
                    .check_version(field, delta.data, &value)
                    .map_err(failed)?;
            }
            value
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
            op: None,
            parked: None,
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
        crate::database::stored_crdts::record(
            datastore,
            crate::database::stored_crdts::custom([state.kind]),
        )
        .await
        .map_err(storage)?;
        datastore
            .merge(&collection.doc_key(doc_short_id), &operand)
            .await
            .map_err(storage)?;
        Ok(true)
    }

    pub(super) async fn stored_block(&self, cid: &Cid) -> std::result::Result<Block, MergeError> {
        let bytes = self
            .blockstore
            .get(cid)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?
            .ok_or_else(|| MergeError::Storage(format!("block {cid} not found in blockstore")))?;
        Block::from_dag_cbor(&bytes).map_err(|e| MergeError::BlockDecode(e.to_string()))
    }

    /// The value after merging the block `cid`, carrying `data`, into
    /// `current`, the value of every other block under `tips` (the field's
    /// heads and `cid`): from the field's last critical block when the type
    /// can, from its whole history otherwise.
    #[allow(clippy::too_many_arguments)]
    async fn concurrent_value(
        &self,
        current: Option<&NormalValue>,
        tips: &[Cid],
        cid: &Cid,
        causal: &dyn CausalCrdt,
        field: &FieldDescription,
        data: &[u8],
        metadata: Option<&BlockMetadata<'_>>,
    ) -> std::result::Result<NormalValue, MergeError> {
        let failed = |error: crdt::FieldCrdtError| MergeError::MergeFailed(error.to_string());
        if let Some((critical, after)) = self
            .critical_block(tips, cid, causal, field, data, metadata)
            .await?
        {
            let merged = causal
                .merge_from(field, current, &critical, &after, &cid.to_bytes())
                .map_err(failed)?;
            if let Some(value) = merged {
                return Ok(value);
            }
        }
        let history = self
            .causal_history(tips, cid, causal, field, data, metadata)
            .await?;
        causal.replay(field, &history).map_err(failed)
    }

    /// The field's last critical block below `tips`: one every other block
    /// under them comes before or after. The walk goes down from `tips`,
    /// highest priority first, and stops when one block is left to visit and
    /// none visited was a root. Then every block visited sits above that one,
    /// and every block not visited below it, whatever the priorities say;
    /// they only decide how soon the walk gets there.
    ///
    /// Returns that block's data, decrypted, and the blocks visited, or `None`
    /// when the walk reaches a root first.
    async fn critical_block(
        &self,
        tips: &[Cid],
        cid: &Cid,
        causal: &dyn CausalCrdt,
        field: &FieldDescription,
        data: &[u8],
        metadata: Option<&BlockMetadata<'_>>,
    ) -> std::result::Result<Option<(Vec<u8>, Vec<CausalBlock>)>, MergeError> {
        let mut seen: RapidHashSet<Cid> = RapidHashSet::default();
        let mut waiting: RapidHashMap<Cid, Block> = RapidHashMap::default();
        let mut frontier: BTreeSet<(u64, Cid)> = BTreeSet::new();
        for tip in tips {
            if seen.insert(*tip) {
                let block = self.stored_block(tip).await?;
                frontier.insert((block.delta.priority(), *tip));
                waiting.insert(*tip, block);
            }
        }
        let mut after = Vec::new();
        while let Some(&(_, next)) = frontier.last() {
            if frontier.len() == 1 && !after.is_empty() {
                if next == *cid {
                    return Ok(None);
                }
                // Only a block merged on this node while its parents held every head
                // can be critical, and that merge ran `check_version` on it, so
                // `merge_from` can trust the length it records.
                debug_assert!(
                    self.is_critical(tips, &next).await?,
                    "the walk picked {next}, which is not critical under {tips:?}"
                );
                let critical = self.block_data(&waiting[&next], metadata).await?;
                return Ok(Some((critical, after)));
            }
            frontier.pop_last();
            let block = waiting.remove(&next).ok_or_else(|| {
                MergeError::MergeFailed(format!("causal block {next} visited twice"))
            })?;
            let parents = block.heads.clone().unwrap_or_default();
            if parents.is_empty() {
                return Ok(None);
            }
            for parent in &parents {
                if seen.insert(*parent) {
                    let parent_block = self.stored_block(parent).await?;
                    frontier.insert((parent_block.delta.priority(), *parent));
                    waiting.insert(*parent, parent_block);
                }
            }
            after.push(
                self.causal_block(next, &block, cid, causal, field, data, metadata)
                    .await?,
            );
        }
        Ok(None)
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
        let mut seen: RapidHashSet<Cid> = tips.iter().copied().collect();
        let mut stack = tips.to_vec();
        let mut history = Vec::new();
        while let Some(next) = stack.pop() {
            let block = self.stored_block(&next).await?;
            for parent in block.heads.iter().flatten() {
                if seen.insert(*parent) {
                    stack.push(*parent);
                }
            }
            history.push(
                self.causal_block(next, &block, cid, causal, field, data, metadata)
                    .await?,
            );
        }
        Ok(history)
    }

    /// The stored block `id` of a causal field's history, with the operation
    /// it makes. The block `cid` being merged carries `data`, already
    /// decrypted.
    #[allow(clippy::too_many_arguments)]
    async fn causal_block(
        &self,
        id: Cid,
        block: &Block,
        cid: &Cid,
        causal: &dyn CausalCrdt,
        field: &FieldDescription,
        data: &[u8],
        metadata: Option<&BlockMetadata<'_>>,
    ) -> std::result::Result<CausalBlock, MergeError> {
        let failed = |error: crdt::FieldCrdtError| MergeError::BlockDecode(error.to_string());
        let op = if id == *cid {
            causal.block_op(field, data).map_err(failed)?
        } else {
            let writer = self
                .db
                .crdts()
                .delta(&block.delta, field.crdt_type)
                .map_err(|error| MergeError::UnsupportedDelta(error.to_string()))?;
            crdt::history_op(writer, field, &self.block_data(block, metadata).await?)
                .map_err(failed)?
        };
        Ok(CausalBlock {
            id: id.to_bytes(),
            parents: block.heads.iter().flatten().map(Cid::to_bytes).collect(),
            op,
        })
    }

    /// A stored field block's data, decrypted.
    pub(super) async fn block_data(
        &self,
        block: &Block,
        metadata: Option<&BlockMetadata<'_>>,
    ) -> std::result::Result<Vec<u8>, MergeError> {
        let stored = block
            .delta
            .field()
            .ok_or_else(|| MergeError::UnsupportedDelta("not a field delta".into()))?;
        self.decrypt_block_data(stored.data, block.encryption.as_ref(), metadata)
            .await
    }
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// Whether `critical` is critical under `tips`, checked apart from the
    /// walk in [`critical_block`](Self::critical_block).
    async fn is_critical(
        &self,
        tips: &[Cid],
        critical: &Cid,
    ) -> std::result::Result<bool, MergeError> {
        let below = self.history_parents(std::slice::from_ref(critical)).await?;
        let all = self.history_parents(tips).await?;
        Ok(is_critical(&all, &below, critical))
    }

    /// Every block under `starts`, `starts` included, with its parents.
    async fn history_parents(
        &self,
        starts: &[Cid],
    ) -> std::result::Result<RapidHashMap<Cid, Vec<Cid>>, MergeError> {
        let mut parents: RapidHashMap<Cid, Vec<Cid>> = RapidHashMap::default();
        let mut stack = starts.to_vec();
        while let Some(next) = stack.pop() {
            if parents.contains_key(&next) {
                continue;
            }
            let heads = self.stored_block(&next).await?.heads.unwrap_or_default();
            stack.extend(heads.iter().copied());
            parents.insert(next, heads);
        }
        Ok(parents)
    }
}

/// Whether every block of `all` (each with its parents) comes before or after
/// `critical`. `below` holds `critical` and every block before it. Every
/// other block has to sit above it: it has parents, and each is `critical` or
/// another block outside `below`, so every path down from it reaches
/// `critical`.
fn is_critical<K>(
    all: &RapidHashMap<K, Vec<K>>,
    below: &RapidHashMap<K, Vec<K>>,
    critical: &K,
) -> bool
where
    K: Eq + std::hash::Hash,
{
    all.iter()
        .filter(|(id, _)| !below.contains_key(*id))
        .all(|(_, parents)| {
            !parents.is_empty()
                && parents
                    .iter()
                    .all(|parent| parent == critical || !below.contains_key(parent))
        })
}

#[cfg(test)]
#[path = "field_causal_tests.rs"]
mod tests;
