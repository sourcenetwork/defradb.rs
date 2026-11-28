//! A peer's chunk of a protected parent is an update to that parent: its
//! signer must be allowed to update the parent. `proofs/tla/ChunkSigner.tla`
//! checks this.

use super::composite::{CompositeMergeContext, CompositeMergePreparation};
use super::protected_update::Refusal;
use super::*;
use crate::merge::governance::{Deferral, WaitKey};
use schema::CHUNK_SOURCE_ID_FIELD;

/// The status a chunk is judged with: an update of its parent.
const UPDATE_STATUS: u8 = 1;

/// The composite frame a chunk merge prepares.
pub(super) struct ChunkFrame<'a, 'b> {
    pub(super) cid: &'a Cid,
    pub(super) block: &'a Block,
    pub(super) payload: &'a defra_core::block::CompositeDeltaPayload,
    pub(super) metadata: &'a BlockMetadata<'b>,
    pub(super) doc_id: &'a str,
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// Refuses, in the merge's transaction, a peer's new chunk whose parent is
    /// protected, unless the parent has arrived or is deleted here and the
    /// chunk's signer may update it. A provider that cannot join the
    /// transaction judges the signer before it, in
    /// [`prepare_chunk_merge`](Self::prepare_chunk_merge).
    pub(crate) async fn check_chunk_signer(
        &self,
        datastore: &NamespaceView,
        systemstore: &NamespaceView,
        context: &CompositeMergeContext<'_, '_>,
        chunks: &Collection,
        chunk: &Document,
    ) -> std::result::Result<(), MergeError> {
        // An encrypted chunk this node can read has its parent decrypted into
        // the document; fall back to the signed link when the field was skipped.
        let parent_id = match chunk
            .get(CHUNK_SOURCE_ID_FIELD)
            .and_then(NormalValue::as_str)
        {
            Some(parent) => Some(parent.to_owned()),
            None => self.linked_parent(context.block, context.metadata).await?,
        };
        // A chunk this node cannot read names no parent and shows nothing.
        let Some(parent_id) = parent_id else {
            return Ok(());
        };
        let Some(hook) = self
            .composite_merge_hook()
            .filter(|hook| hook.guards_protected_updates())
        else {
            return Ok(());
        };
        let Some(parent) = crate::chunk::access::parent_collection(&self.db, chunks.schema())
            .map_err(MergeError::Database)?
        else {
            return Err(MergeError::MergeFailed(
                "the chunk's parent collection is not known here".into(),
            ));
        };
        if parent.schema().policy.is_none() {
            return Ok(());
        }
        match parent_state(datastore, systemstore, &parent, &parent_id).await? {
            ParentState::Missing => {
                return Err(MergeError::MergeFailed(
                    "the chunk's parent has not arrived".into(),
                ))
            }
            // A deleted parent shows no chunk, so its chunks decide nothing.
            ParentState::Deleted => return Ok(()),
            ParentState::Live if !hook.supports_transactions() => return Ok(()),
            ParentState::Live => {}
        }
        let view = systemstore.sibling(storage::namespace::Namespace::Acpstore);
        let refusal = self
            .check_signed_update(
                context.cid,
                context.block,
                UPDATE_STATUS,
                &parent_id,
                parent.schema(),
                Some(view),
            )
            .await?;
        match refusal.map(Refusal::into_outcome) {
            None => Ok(()),
            Some(MergeOutcome::Rejected { reason }) => Err(MergeError::ChunkRejected(format!(
                "its signer may not update its parent: {reason}"
            ))),
            Some(outcome) => Err(MergeError::MergeFailed(format!(
                "the chunk's parent update check did not pass: {outcome:?}"
            ))),
        }
    }

    /// Before its transaction opens, a peer's new chunk of a protected parent
    /// that has not arrived waits for it, indexed under the parent's DocID so
    /// the parent's first merge re-drives it. A provider that cannot join the
    /// transaction judges the chunk's signer here. The parent is the one the
    /// chunk's unencrypted first block names.
    pub(super) async fn prepare_chunk_merge(
        &self,
        frame: ChunkFrame<'_, '_>,
        chunks: &CollectionVersion,
    ) -> std::result::Result<Option<CompositeMergePreparation>, MergeError> {
        let ChunkFrame {
            cid,
            block,
            payload,
            metadata,
            doc_id,
        } = frame;
        let genesis = block.heads.as_deref().is_none_or(<[Cid]>::is_empty);
        if chunks.chunk_of.is_none() || !genesis || payload.status == 2 {
            return Ok(None);
        }
        let Some(hook) = self
            .composite_merge_hook()
            .filter(|hook| hook.guards_protected_updates())
        else {
            return Ok(None);
        };
        let Some(parent) = crate::chunk::access::parent_collection(&self.db, chunks)
            .map_err(MergeError::Database)?
        else {
            return Ok(None);
        };
        let Some(policy) = parent.schema().policy.as_ref() else {
            return Ok(None);
        };
        let Some(parent_id) = self.linked_parent(block, metadata).await? else {
            return Ok(None);
        };
        if !self.parent_held(&parent, &parent_id).await? {
            let awaited = WaitKey::Document(parent_id.clone());
            self.index_deferred(
                cid,
                doc_id,
                metadata,
                Deferral::Awaiting(vec![awaited.clone()]),
            );
            // The parent may have merged, and released its waiters, before
            // the chunk was indexed: then the chunk re-drives itself.
            if self.parent_held(&parent, &parent_id).await? {
                self.deferred.release([awaited]);
            }
            return Ok(Some(CompositeMergePreparation::Complete(
                MergeOutcome::retryable_skip("the chunk's parent has not arrived"),
            )));
        }
        if hook.supports_transactions() {
            return Ok(None);
        }
        let refusal = self
            .check_signed_update(cid, block, UPDATE_STATUS, &parent_id, parent.schema(), None)
            .await?;
        Ok(refusal.map(|refusal| match refusal {
            Refusal::Outcome(outcome) => CompositeMergePreparation::Complete(outcome),
            Refusal::Parked(outcome) => CompositeMergePreparation::Deferred {
                outcome,
                awaiting: Deferral::Parked {
                    object: WaitKey::acp_object(&policy.id, &policy.resource_name, &parent_id),
                    refused: *cid,
                },
            },
        }))
    }

    /// Whether this node holds the parent, live or deleted, in a snapshot read
    /// that only decides whether the chunk waits: the merge's transaction
    /// checks again.
    async fn parent_held(
        &self,
        parent: &Collection,
        parent_id: &str,
    ) -> std::result::Result<bool, MergeError> {
        let txn = self.db.new_txn(true).await.map_err(MergeError::Database)?;
        let state = parent_state(
            &txn.datastore().map_err(MergeError::Database)?,
            &txn.systemstore().map_err(MergeError::Database)?,
            parent,
            parent_id,
        )
        .await;
        txn.force_discard().map_err(MergeError::Database)?;
        Ok(!matches!(state?, ParentState::Missing))
    }

    /// The parent a chunk's first block names in its unencrypted parent field.
    async fn linked_parent(
        &self,
        block: &Block,
        metadata: &BlockMetadata<'_>,
    ) -> std::result::Result<Option<String>, MergeError> {
        for link in block.links.iter().flatten() {
            if link.name != CHUNK_SOURCE_ID_FIELD {
                continue;
            }
            let Some(data) = self
                .blockstore
                .get(&link.link)
                .await
                .map_err(|error| MergeError::Storage(error.to_string()))?
            else {
                return Ok(None);
            };
            let mut linked = Block::from_dag_cbor(&data)
                .map_err(|error| MergeError::BlockDecode(error.to_string()))?;
            if let Some(encryption) = linked.encryption {
                let Some(data) = linked.delta.field_data_mut() else {
                    return Ok(None);
                };
                *data = self
                    .decrypt_block_data(data, Some(&encryption), Some(metadata))
                    .await?;
            }
            let mut fields = crate::replication_filter::FieldValues::default();
            crate::replication_filter::add_field_values(&mut fields, &linked);
            return Ok(match fields.remove(CHUNK_SOURCE_ID_FIELD) {
                Some(ciborium::Value::Text(parent)) => Some(parent),
                _ => None,
            });
        }
        Ok(None)
    }
}

/// What this node holds of a chunk's parent.
enum ParentState {
    Missing,
    Deleted,
    Live,
}

/// A merge can map the parent's ID before it stores the parent, so a parent
/// only mapped is missing.
async fn parent_state(
    datastore: &NamespaceView,
    systemstore: &NamespaceView,
    parent: &Collection,
    parent_id: &str,
) -> std::result::Result<ParentState, MergeError> {
    let Some(parent_ref) = crate::docid::map::get_doc_ref(systemstore, parent_id)
        .await
        .map_err(MergeError::Database)?
    else {
        return Ok(ParentState::Missing);
    };
    if parent
        .is_deleted(datastore, parent_ref.doc_short_id)
        .await
        .map_err(MergeError::Database)?
    {
        return Ok(ParentState::Deleted);
    }
    let stored = parent
        .read_hint(datastore, parent_ref.doc_short_id)
        .await
        .map_err(MergeError::Database)?
        .is_some();
    Ok(if stored {
        ParentState::Live
    } else {
        ParentState::Missing
    })
}
