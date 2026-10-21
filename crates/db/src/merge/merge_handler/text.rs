//! Merges a Text field's block into the field's stored text.
//!
//! The stored text is the replay of every block under the field's heads. A
//! block whose parents are those heads was written against exactly that text,
//! so its ops apply to it as plain edits. Any other block replays the field's
//! whole DAG with the block added, which also makes a block merged twice a
//! no-op. The replay walks the whole history, not just back to a common
//! ancestor: Fugue places a character next to deleted ones, which the stored
//! text no longer holds.

use super::*;
use crdt::TextBlock;
use defra_core::TextOp;

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

        let text = if heads.contains(cid) {
            current
        } else if sorted_parents == heads {
            let ops = TextOp::decode_all(&payload.data)
                .map_err(|e| MergeError::BlockDecode(e.to_string()))?;
            TextOp::apply_all(&current, &ops)
        } else {
            heads.push(*cid);
            let history = self.text_history(&heads, cid, payload, metadata).await?;
            crdt::replay_text(&history).map_err(|e| MergeError::MergeFailed(e.to_string()))?
        };
        Ok(TextMergeResult {
            value: NormalValue::String(text),
            priority: field_priority.max(payload.priority),
        })
    }

    /// Every block of a Text field's DAG under `tips`. The block `cid` being
    /// merged comes from `payload`, already decrypted; the others are read
    /// from the blockstore, which holds them since a block merges only after
    /// its parents are local.
    async fn text_history(
        &self,
        tips: &[Cid],
        cid: &Cid,
        payload: &defra_core::block::TextDeltaPayload,
        metadata: &BlockMetadata<'_>,
    ) -> std::result::Result<Vec<TextBlock>, MergeError> {
        let mut seen: RapidHashSet<Cid> = tips.iter().copied().collect();
        let mut stack = tips.to_vec();
        let mut history = Vec::new();
        while let Some(next) = stack.pop() {
            let bytes = self
                .blockstore
                .get(&next)
                .await
                .map_err(|e| MergeError::Storage(e.to_string()))?
                .ok_or_else(|| {
                    MergeError::Storage(format!("Text block {next} not found in blockstore"))
                })?;
            let block =
                Block::from_dag_cbor(&bytes).map_err(|e| MergeError::BlockDecode(e.to_string()))?;
            let parents = block.heads.clone().unwrap_or_default();
            let ops = if next == *cid {
                TextOp::decode_all(&payload.data)
            } else {
                self.text_ops_of(&block, metadata).await?
            }
            .map_err(|e| MergeError::BlockDecode(e.to_string()))?;
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

    /// The edits a stored block of a Text field makes: its ops, or, for a
    /// value block, a replacement of the whole text.
    async fn text_ops_of(
        &self,
        block: &Block,
        metadata: &BlockMetadata<'_>,
    ) -> std::result::Result<defra_core::Result<Vec<TextOp>>, MergeError> {
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
        Ok(match &block.delta {
            CrdtDelta::Text(_) => TextOp::decode_all(&data),
            _ => {
                let value = if data.is_empty() {
                    NormalValue::Null
                } else {
                    ciborium::from_reader::<NormalValue, _>(data.as_slice())
                        .map_err(|e| MergeError::BlockDecode(e.to_string()))?
                };
                Ok(TextOp::replace_all(value.as_str().unwrap_or_default()))
            }
        })
    }
}
