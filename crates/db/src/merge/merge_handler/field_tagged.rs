//! A tagged field's merge ([`crdt::TaggedCrdt`]): a block's operand acts only
//! on the operations its block had seen, so it lands blind on the stored
//! state, which is read without validation, and concurrent merges commute.
//! A block whose parents are the field's heads has seen every operation the
//! state records. Any other block walks down its own ancestors, highest
//! priority first, until every recorded operation is found among them or has
//! a higher priority than every block left to visit, which no ancestor can
//! match.
use std::collections::BTreeSet;

use super::*;
use crdt::field::tagged::{encode_operand, land, Tag};
use crdt::TaggedCrdt;
use defra_core::block::FieldDelta;
use document::FieldOp;

use super::field::FieldMergeResult;

impl<S, B> DbMergeHandler<S, B>
where
    S: Store,
    B: blockstore::Blockstore,
{
    /// Merge the block `cid` of the tagged `field` into its stored state. A
    /// block among the field's heads, or whose operation the state records,
    /// changes nothing. The operand lands on the field's part of the document
    /// value; a value on another schema version is rewritten whole by the
    /// block's composite, which takes the operand. A tagged block always
    /// merges through its composite, never standalone.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn merge_field_tagged(
        &self,
        datastore: &mut NamespaceView,
        headstore: &NamespaceView,
        cid: &Cid,
        tagged: &dyn TaggedCrdt,
        collection: &Collection,
        field: &FieldDescription,
        delta: &FieldDelta<'_>,
        doc_short_id: u64,
        parents: Option<&[Cid]>,
        metadata: Option<&BlockMetadata<'_>>,
        reads: &mut super::block_reads::BlockReads,
    ) -> std::result::Result<FieldMergeResult, MergeError> {
        let failed = |error: crdt::FieldCrdtError| MergeError::MergeFailed(error.to_string());
        let target =
            super::field_ops::op_target(datastore, collection, doc_short_id, field).await?;
        let state = collection
            .read_part_unvalidated(datastore, doc_short_id, &field.name)
            .await
            .map_err(MergeError::Database)?;
        let mut heads: Vec<Cid> = reads
            .heads(headstore, doc_short_id)
            .await
            .map_err(|e| MergeError::Storage(e.to_string()))?
            .live()
            .filter_map(|(name, _)| crate::block::heads::document::parse_name(name))
            .filter(|(name, _)| *name == field.name)
            .map(|(_, head)| head)
            .collect();
        heads.sort_unstable();
        let mut parents = match parents {
            Some(parents) => parents.to_vec(),
            None => self.stored_block(cid).await?.heads.unwrap_or_default(),
        };
        parents.sort_unstable();
        let parent_ids: Vec<Vec<u8>> = parents.iter().map(Cid::to_bytes).collect();
        let id = tagged.op_id(delta.priority, &parent_ids, delta.data);
        let tags = match &state {
            Some(state) => tagged.tags(field, state.part()).map_err(failed)?,
            None => Vec::new(),
        };
        let current = |state: Option<&crdt::stored::StoredField>| {
            state.and_then(|state| tagged.materialize(state.part()))
        };
        if heads.contains(cid) || tags.iter().any(|tag| tag.id == id) {
            return Ok(FieldMergeResult {
                applied: false,
                value: current(state.as_ref()),
                order: None,
                deferred: false,
                op: None,
            });
        }
        let part = state.as_ref().map(|state| state.part());
        let operand = if parents == heads {
            tagged.effect(field, part, delta.data, &id, delta.priority, &|_| true)
        } else {
            let past = self
                .tagged_past(&parents, tags, tagged, field, metadata)
                .await?;
            tagged.effect(field, part, delta.data, &id, delta.priority, &|tag| {
                past.contains(tag)
            })
        }
        .map_err(failed)?;
        let landed = land(tagged, state.as_ref(), operand.clone()).ok_or_else(|| {
            MergeError::MergeFailed(format!("invalid operand on '{}'", field.name))
        })?;
        let op = match target {
            Some(part) => {
                let bytes =
                    storage::merge::document_part(part, operand.part()).ok_or_else(|| {
                        MergeError::MergeFailed(format!("invalid operand on '{}'", field.name))
                    })?;
                datastore
                    .merge(&collection.doc_key(doc_short_id), &bytes)
                    .await
                    .map_err(|e| MergeError::Storage(e.to_string()))?;
                None
            }
            None => Some(FieldOp::Operand {
                data: delta.data.to_vec(),
                operand: encode_operand(&operand).ok_or_else(|| {
                    MergeError::MergeFailed(format!("invalid operand on '{}'", field.name))
                })?,
            }),
        };
        Ok(FieldMergeResult {
            applied: true,
            value: current(Some(&landed)),
            order: None,
            deferred: false,
            op,
        })
    }

    /// The IDs, among `tags`, of the operations made below `parents`: the walk
    /// goes down from `parents`, highest priority first, until no block left
    /// could have made one.
    async fn tagged_past(
        &self,
        parents: &[Cid],
        tags: Vec<Tag>,
        tagged: &dyn TaggedCrdt,
        field: &FieldDescription,
        metadata: Option<&BlockMetadata<'_>>,
    ) -> std::result::Result<RapidHashSet<Vec<u8>>, MergeError> {
        let mut unresolved: BTreeSet<Tag> = tags.into_iter().collect();
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
                .is_none_or(|lowest| lowest.priority > priority)
            {
                break;
            }
            let block = self.stored_block(&next).await?;
            let block_parents = block.heads.clone().unwrap_or_default();
            let writer = block
                .delta
                .field()
                .and_then(|_| crdt::delta_crdt(&block.delta, field.crdt_type).ok());
            if writer.is_some_and(|writer| writer.tagged().is_some()) {
                let data = self.block_data(&block, metadata).await?;
                let parent_ids: Vec<Vec<u8>> = block_parents.iter().map(Cid::to_bytes).collect();
                let id = tagged.op_id(block.delta.priority(), &parent_ids, &data);
                unresolved.retain(|tag| tag.id != id);
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
}
