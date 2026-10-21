//! Merges a Json field's block into the field's stored frontier.
//!
//! The block's operand removes the entries in its causal past at its paths
//! and below, and records the ones above them it had seen. A block whose
//! parents are the field's heads has every stored entry in its past. Any
//! other block walks down its own DAG, highest priority first, until every
//! stored entry is either found among its ancestors or has a priority above
//! every block left to visit, which no ancestor can match.

use std::collections::BTreeSet;

use super::*;
use defra_core::json_crdt::{self, Frontier};

fn json_value(frontier: &Frontier) -> NormalValue {
    frontier
        .materialize()
        .map_or(NormalValue::Null, NormalValue::Json)
}

/// What a Json block's merge leaves: the operand on the stored frontier, the
/// value after it, and the priority the field reaches.
pub(crate) struct JsonMergeResult {
    pub(crate) delta: Option<Frontier>,
    pub(crate) value: NormalValue,
    pub(crate) priority: u64,
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// Merge the Json block `cid`, whose parents are `parents`, into its
    /// field. `payload.data` is already decrypted.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn process_json_delta_in_txn(
        &self,
        datastore: &NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        parents: &[Cid],
        payload: &defra_core::block::JsonDeltaPayload,
        fallback_collection_id: Option<&str>,
        doc_short_id: u64,
        metadata: &BlockMetadata<'_>,
        reads: &mut super::block_reads::BlockReads,
    ) -> std::result::Result<JsonMergeResult, MergeError> {
        let field = payload.field_name.as_str();
        let collection = self
            .block_collection(&payload.schema_version_id, fallback_collection_id)
            .await?
            .ok_or_else(|| {
                MergeError::MergeFailed(format!("no collection for Json field {field}"))
            })?;
        let mut frontier = collection
            .read_json_frontier(datastore, doc_short_id, field)
            .await?;
        let field_priority = reads
            .field_priority(headstore, doc_short_id, field)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?
            .max(payload.priority);
        let mut heads: Vec<Cid> = reads
            .heads(headstore, doc_short_id)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?
            .live()
            .filter_map(|(name, _)| crate::block::heads::document::parse_name(name))
            .filter(|(name, _)| *name == field)
            .map(|(_, head)| head)
            .collect();
        heads.sort_unstable();
        let parent_bytes: Vec<Vec<u8>> = parents.iter().map(Cid::to_bytes).collect();
        let id = json_crdt::block_id(payload.priority, &parent_bytes, &payload.data);
        if heads.contains(cid) || frontier.entries.iter().any(|entry| entry.block == id) {
            return Ok(JsonMergeResult {
                delta: None,
                value: json_value(&frontier),
                priority: field_priority,
            });
        }
        let ops = json_crdt::decode_ops(&payload.data)
            .map_err(|e| MergeError::BlockDecode(e.to_string()))?;
        let mut sorted_parents = parents.to_vec();
        sorted_parents.sort_unstable();
        let delta = if sorted_parents == heads {
            frontier.local_delta(&ops, &id, payload.priority)
        } else {
            let past = self.json_past(parents, &frontier, metadata).await?;
            frontier.delta(&ops, &id, payload.priority, |block| past.contains(block))
        };
        frontier.compose(delta.clone());
        Ok(JsonMergeResult {
            delta: Some(delta),
            value: json_value(&frontier),
            priority: field_priority,
        })
    }

    /// The IDs of the blocks below `parents` that `frontier` names, found by
    /// walking down from `parents` until no block left could be one.
    async fn json_past(
        &self,
        parents: &[Cid],
        frontier: &Frontier,
        metadata: &BlockMetadata<'_>,
    ) -> std::result::Result<RapidHashSet<Vec<u8>>, MergeError> {
        let mut unresolved: BTreeSet<(u64, &[u8])> = frontier
            .entries
            .iter()
            .map(|entry| (entry.priority, entry.block.as_slice()))
            .collect();
        let mut past = RapidHashSet::default();
        let mut seen: RapidHashSet<Cid> = RapidHashSet::default();
        let mut waiting: BTreeSet<(u64, Cid)> = BTreeSet::new();
        for parent in parents {
            if seen.insert(*parent) {
                let block = self.stored_block(parent).await?;
                waiting.insert((block.delta.priority(), *parent));
            }
        }
        while let Some((priority, next)) = waiting.pop_last() {
            if unresolved
                .first()
                .is_none_or(|(lowest, _)| *lowest > priority)
            {
                break;
            }
            let block = self.stored_block(&next).await?;
            let block_parents = block.heads.clone().unwrap_or_default();
            if let CrdtDelta::Json(payload) = &block.delta {
                let data = self
                    .decrypt_block_data(&payload.data, block.encryption.as_ref(), Some(metadata))
                    .await?;
                let parent_bytes: Vec<Vec<u8>> = block_parents.iter().map(Cid::to_bytes).collect();
                let id = json_crdt::block_id(payload.priority, &parent_bytes, &data);
                unresolved.retain(|(_, block)| *block != id.as_slice());
                past.insert(id);
            }
            for parent in &block_parents {
                if seen.insert(*parent) {
                    let parent_block = self.stored_block(parent).await?;
                    waiting.insert((parent_block.delta.priority(), *parent));
                }
            }
        }
        Ok(past)
    }

    /// The stored block `cid` of a field's DAG.
    async fn stored_block(&self, cid: &Cid) -> std::result::Result<Block, MergeError> {
        let bytes = self
            .blockstore
            .get(cid)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?
            .ok_or_else(|| MergeError::Storage(format!("block {cid} not found in blockstore")))?;
        Block::from_dag_cbor(&bytes).map_err(|e| MergeError::BlockDecode(e.to_string()))
    }
}
