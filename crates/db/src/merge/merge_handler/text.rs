//! Merges a Text field's block into the field's stored text.
//!
//! The stored text is the replay of every block under the field's heads. A
//! block whose parents are those heads was written against exactly that text,
//! so its ops apply to it as plain edits. Any other block is concurrent with
//! something already merged: the merge walks down from the heads and the block
//! to the field's last critical block (one every other block comes before or
//! after) and replays only the blocks above it, over placeholders for its
//! text. When no such block records its text's length, or the walk reaches a
//! root first, the whole DAG is replayed, which also makes a block merged twice
//! a no-op.
//!
//! The placeholders need the critical block's text length, which every Text
//! block records. Whenever a merge's result is the block's own version (its
//! parents hold every block already merged), that length is checked, and a
//! block only ever becomes critical on a node after merging that way there.

use std::collections::BTreeSet;

use super::*;
use crdt::TextBlock;
use defra_core::{TextEdits, TextOp};

/// The text a Text block's merge leaves, at the priority the field reaches.
pub(crate) struct TextMergeResult {
    pub(crate) value: NormalValue,
    pub(crate) priority: u64,
}

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// Merge the Text block `cid`, whose parents are `parents`, into its
    /// field. `payload.data` is already decrypted.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn process_text_delta_in_txn(
        &self,
        datastore: &NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        parents: &[Cid],
        payload: &defra_core::block::TextDeltaPayload,
        fallback_collection_id: Option<&str>,
        doc_id_str: &str,
        doc_short_id: u64,
        metadata: &BlockMetadata<'_>,
        reads: &mut super::block_reads::BlockReads,
    ) -> std::result::Result<TextMergeResult, MergeError> {
        let field = payload.field_name.as_str();
        let collection = self
            .block_collection(&payload.schema_version_id, fallback_collection_id)
            .await?
            .ok_or_else(|| {
                MergeError::MergeFailed(format!("no collection for Text field {field}"))
            })?;
        let doc_id = DocID::from_string(doc_id_str)
            .map_err(|e| MergeError::MergeFailed(format!("invalid doc_id: {e}")))?;
        // The stored text decides the result, so this read is validated: a
        // concurrent writer of the field aborts the merge, which runs again.
        let current = reads
            .document(&collection, datastore, doc_short_id, &doc_id)
            .await?
            .and_then(|doc| {
                doc.get(field)
                    .and_then(NormalValue::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        let field_priority = reads
            .field_priority(headstore, doc_short_id, field)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?;
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
        let mut sorted_parents = parents.to_vec();
        sorted_parents.sort_unstable();
        if heads.contains(cid) {
            return Ok(TextMergeResult {
                value: NormalValue::String(current),
                priority: field_priority.max(payload.priority),
            });
        }

        let edits =
            TextEdits::decode(&payload.data).map_err(|e| MergeError::BlockDecode(e.to_string()))?;
        let text = if sorted_parents == heads {
            TextOp::apply_all(&current, &edits.ops)
        } else {
            let mut tips = heads.clone();
            tips.push(*cid);
            self.concurrent_text(&current, &tips, cid, &edits, metadata)
                .await?
        };
        // With every merged block among its parents, the result is the
        // block's own version, so the length it records has to be that text's.
        if heads
            .iter()
            .all(|head| sorted_parents.binary_search(head).is_ok())
        {
            let len = text.chars().count() as u64;
            if len != edits.len {
                return Err(MergeError::MergeFailed(format!(
                    "Text block {cid} of field {field} records {} characters for its version, \
                     which has {len}",
                    edits.len
                )));
            }
        }
        Ok(TextMergeResult {
            value: NormalValue::String(text),
            priority: field_priority.max(payload.priority),
        })
    }

    /// The text after merging the Text block `cid`, whose edits are `edits`,
    /// into `current`, the text of every other block under `tips` (the field's
    /// heads and `cid`).
    async fn concurrent_text(
        &self,
        current: &str,
        tips: &[Cid],
        cid: &Cid,
        edits: &TextEdits,
        metadata: &BlockMetadata<'_>,
    ) -> std::result::Result<String, MergeError> {
        if let Some((base, after)) = self.critical_block(tips, cid, edits, metadata).await? {
            let merged = crdt::merge_text(current, base, &after, &cid.to_bytes())
                .map_err(|e| MergeError::MergeFailed(e.to_string()))?;
            if let Some(text) = merged {
                return Ok(text);
            }
        }
        let history = self.text_history(tips, cid, edits, metadata).await?;
        crdt::replay_text(&history).map_err(|e| MergeError::MergeFailed(e.to_string()))
    }

    /// The field's last critical block below `tips`: one every other block
    /// under them comes before or after. The walk goes down from `tips`,
    /// highest priority first, and stops when one block is left to visit and
    /// none visited was a root. Then every block visited sits above that one,
    /// and every block not visited below it, whatever the priorities say;
    /// they only decide how soon the walk gets there.
    ///
    /// Returns the length of that block's text and the blocks visited, or
    /// `None` when the walk reaches a root first.
    async fn critical_block(
        &self,
        tips: &[Cid],
        cid: &Cid,
        edits: &TextEdits,
        metadata: &BlockMetadata<'_>,
    ) -> std::result::Result<Option<(usize, Vec<TextBlock>)>, MergeError> {
        let mut seen: RapidHashSet<Cid> = RapidHashSet::default();
        let mut waiting: RapidHashMap<Cid, Block> = RapidHashMap::default();
        let mut frontier: BTreeSet<(u64, Cid)> = BTreeSet::new();
        for tip in tips {
            if seen.insert(*tip) {
                let block = self.text_block(tip).await?;
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
                let below = self.text_edits_of(&waiting[&next], metadata).await?;
                return Ok(usize::try_from(below.len).ok().map(|len| (len, after)));
            }
            frontier.pop_last();
            let block = waiting.remove(&next).ok_or_else(|| {
                MergeError::MergeFailed(format!("Text block {next} visited twice"))
            })?;
            let parents = block.heads.clone().unwrap_or_default();
            if parents.is_empty() {
                return Ok(None);
            }
            let ops = if next == *cid {
                edits.ops.clone()
            } else {
                self.text_edits_of(&block, metadata).await?.ops
            };
            for parent in &parents {
                if seen.insert(*parent) {
                    let parent_block = self.text_block(parent).await?;
                    frontier.insert((parent_block.delta.priority(), *parent));
                    waiting.insert(*parent, parent_block);
                }
            }
            after.push(TextBlock {
                id: next.to_bytes(),
                parents: parents.iter().map(Cid::to_bytes).collect(),
                ops,
            });
        }
        Ok(None)
    }

    /// Every block of a Text field's DAG under `tips`. The block `cid` being
    /// merged has `edits`, already decrypted; the others are read from the
    /// blockstore, which holds them since a block merges only after its
    /// parents are local.
    async fn text_history(
        &self,
        tips: &[Cid],
        cid: &Cid,
        edits: &TextEdits,
        metadata: &BlockMetadata<'_>,
    ) -> std::result::Result<Vec<TextBlock>, MergeError> {
        let mut seen: RapidHashSet<Cid> = tips.iter().copied().collect();
        let mut stack = tips.to_vec();
        let mut history = Vec::new();
        while let Some(next) = stack.pop() {
            let block = self.text_block(&next).await?;
            let parents = block.heads.clone().unwrap_or_default();
            let ops = if next == *cid {
                edits.ops.clone()
            } else {
                self.text_edits_of(&block, metadata).await?.ops
            };
            for parent in &parents {
                if seen.insert(*parent) {
                    stack.push(*parent);
                }
            }
            history.push(TextBlock {
                id: next.to_bytes(),
                parents: parents.iter().map(Cid::to_bytes).collect(),
                ops,
            });
        }
        Ok(history)
    }

    /// The stored block `cid` of a Text field's DAG.
    async fn text_block(&self, cid: &Cid) -> std::result::Result<Block, MergeError> {
        let bytes = self
            .blockstore
            .get(cid)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?
            .ok_or_else(|| {
                MergeError::Storage(format!("Text block {cid} not found in blockstore"))
            })?;
        Block::from_dag_cbor(&bytes).map_err(|e| MergeError::BlockDecode(e.to_string()))
    }

    /// The edits a stored block of a Text field makes, and the length of its
    /// version's text: its own, or, for a value block, a replacement of the
    /// whole text by its value.
    async fn text_edits_of(
        &self,
        block: &Block,
        metadata: &BlockMetadata<'_>,
    ) -> std::result::Result<TextEdits, MergeError> {
        let data = match &block.delta {
            CrdtDelta::Text(payload) => &payload.data,
            CrdtDelta::Lww(payload) => &payload.data,
            other => {
                return Err(MergeError::UnsupportedDelta(format!(
                    "a Text field's history holds a {:?} block",
                    std::mem::discriminant(other)
                )))
            }
        };
        let data = self
            .decrypt_block_data(data, block.encryption.as_ref(), Some(metadata))
            .await?;
        match &block.delta {
            CrdtDelta::Text(_) => {
                TextEdits::decode(&data).map_err(|e| MergeError::BlockDecode(e.to_string()))
            }
            _ => {
                let value = if data.is_empty() {
                    NormalValue::Null
                } else {
                    ciborium::from_reader::<NormalValue, _>(data.as_slice())
                        .map_err(|e| MergeError::BlockDecode(e.to_string()))?
                };
                let text = value.as_str().unwrap_or_default();
                Ok(TextEdits {
                    ops: TextOp::replace_all(text),
                    len: text.chars().count() as u64,
                })
            }
        }
    }
}
