//! The verdict on a collection block of a governed collection.
//!
//! A collection block is not a composite, so the validator never judges one,
//! yet installing it as a head claims history: "these composites are current,
//! after these earlier collection blocks". Every composite it links is judged
//! by the validator on its own path, and those verdicts are pure, so the
//! block's verdict is derived from them and is the same on every replica: it
//! installs when every link and every parent has merged, is rejected when any
//! is known rejected, and otherwise waits for what is missing.

use cid::Cid;
use defra_core::block::{Block, CollectionDeltaPayload, CrdtDelta};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeOutcome};
use storage::corekv::Store;

use super::WaitKey;
use crate::collection::Collection;
use crate::merge::merge_handler::{CidSet, DbMergeHandler, MergeError};

/// What the links of a governed collection block say about it.
pub(crate) enum CollectionBlockVerdict {
    /// Every linked composite and every parent has merged: install the head.
    Install,
    /// A link or a parent is known rejected, so the claim can never be true.
    Rejected(MergeOutcome),
    /// A link or a parent is not held, or is held but not yet merged; the
    /// block waits on each such CID.
    Deferred {
        outcome: MergeOutcome,
        awaiting: Vec<WaitKey>,
    },
}

/// The sets a batch merge has merged in its open transaction, not yet visible
/// to the handler's own sets or the blockstore.
#[derive(Clone, Copy)]
pub(crate) struct BatchMerged<'a> {
    pub composites: &'a CidSet,
    pub collections: &'a CidSet,
}

enum LinkState {
    Merged,
    Rejected,
    Pending,
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    /// The collection a collection block names, when the application has
    /// claimed it; `None` for an ungoverned collection, which merges its
    /// collection blocks as it always has.
    pub(crate) async fn governed_collection_of_block(
        &self,
        payload: &CollectionDeltaPayload,
        metadata: &BlockMetadata<'_>,
    ) -> Result<Option<Collection>, MergeError> {
        Ok(self
            .block_collection(&payload.schema_version_id, metadata.collection_id)
            .await?
            .filter(|collection| self.is_governed(collection.schema())))
    }

    /// Whether a collection block has merged: in this process, in the open
    /// batch, or durably, by the blockstore's merged marker. The marker is
    /// what survives a restart, and a node's own blocks carry it from the
    /// start.
    pub(crate) async fn collection_block_merged(
        &self,
        cid: &Cid,
        batch: Option<BatchMerged<'_>>,
    ) -> Result<bool, MergeError> {
        if self.merged_collections.contains_key(cid)
            || batch.is_some_and(|batch| batch.collections.contains_key(cid))
        {
            return Ok(true);
        }
        self.blockstore
            .is_merged(cid)
            .await
            .map_err(|error| MergeError::Storage(error.to_string()))
    }

    async fn composite_merged(
        &self,
        cid: &Cid,
        batch: Option<BatchMerged<'_>>,
    ) -> Result<bool, MergeError> {
        if self.has_merged_composite(cid)
            || batch.is_some_and(|batch| batch.composites.contains_key(cid))
        {
            return Ok(true);
        }
        self.blockstore
            .is_merged(cid)
            .await
            .map_err(|error| MergeError::Storage(error.to_string()))
    }

    /// A link's state, read from what this node knows: the merged sets and
    /// the blockstore's marker, then the in-memory rejected set. After a
    /// restart a rejected link reads as merely unmerged, so the block defers
    /// on it; the sweep re-judges the link, records the reject again, and the
    /// next pass over the block rejects it.
    fn link_state(&self, cid: &Cid, merged: bool, rejected_now: &[Cid]) -> LinkState {
        if merged {
            return LinkState::Merged;
        }
        if self.rejected_governed.contains_key(cid) || rejected_now.contains(cid) {
            return LinkState::Rejected;
        }
        LinkState::Pending
    }

    /// Judge a governed collection block by its links and its parents. It
    /// drives nothing and only reads, so the caller runs it again under the
    /// collection guard before writing the head, and a link rejected in
    /// between is seen.
    ///
    /// `rejected_now` names the linked composites whose merge, driven by the
    /// caller just before, returned a rejection the validator's own set does
    /// not record, such as a unique-constraint violation.
    pub(crate) async fn judge_governed_collection_block(
        &self,
        cid: &Cid,
        block: &Block,
        batch: Option<BatchMerged<'_>>,
        rejected_now: &[Cid],
    ) -> Result<CollectionBlockVerdict, MergeError> {
        let mut awaiting = Vec::new();
        let mut first_pending: Option<String> = None;

        for parent in block.heads.iter().flatten() {
            let merged = self.collection_block_merged(parent, batch).await?;
            match self.link_state(parent, merged, &[]) {
                LinkState::Merged => {}
                LinkState::Rejected => {
                    return Ok(self.reject_collection_block(
                        cid,
                        format!("collection block supersedes rejected collection block {parent}"),
                    ));
                }
                LinkState::Pending => {
                    first_pending.get_or_insert_with(|| {
                        format!("collection block awaits parent collection block {parent}")
                    });
                    awaiting.push(WaitKey::Composite(*parent));
                }
            }
        }

        for link in block.links.iter().flatten() {
            let link_cid = &link.link;
            let merged = self.composite_merged(link_cid, batch).await?;
            // A held link that is not a composite claims nothing about the
            // collection's documents, as on the ungoverned path.
            if !merged && !self.is_composite_link(link_cid).await?.unwrap_or(true) {
                continue;
            }
            match self.link_state(link_cid, merged, rejected_now) {
                LinkState::Merged => {}
                LinkState::Rejected => {
                    return Ok(self.reject_collection_block(
                        cid,
                        format!("collection block links rejected composite {link_cid}"),
                    ));
                }
                LinkState::Pending => {
                    first_pending.get_or_insert_with(|| {
                        format!("collection block awaits composite {link_cid}")
                    });
                    awaiting.push(WaitKey::Composite(*link_cid));
                }
            }
        }

        Ok(match first_pending {
            Some(reason) => CollectionBlockVerdict::Deferred {
                outcome: MergeOutcome::retryable_skip(reason),
                awaiting,
            },
            None => CollectionBlockVerdict::Install,
        })
    }

    /// Whether a held link decodes to a composite; `None` when it is not held.
    async fn is_composite_link(&self, cid: &Cid) -> Result<Option<bool>, MergeError> {
        let Some(data) = self
            .blockstore
            .get(cid)
            .await
            .map_err(|error| MergeError::Storage(error.to_string()))?
        else {
            return Ok(None);
        };
        let block = Block::from_dag_cbor(&data)
            .map_err(|error| MergeError::BlockDecode(error.to_string()))?;
        Ok(Some(matches!(block.delta, CrdtDelta::Composite(_))))
    }

    /// Record the rejection so the sweep leaves the block alone and a child
    /// block that supersedes it is rejected in turn.
    fn reject_collection_block(&self, cid: &Cid, reason: String) -> CollectionBlockVerdict {
        tracing::debug!(%cid, %reason, "Governed collection block rejected");
        self.rejected_governed.insert(*cid, ());
        CollectionBlockVerdict::Rejected(MergeOutcome::rejected(reason))
    }

    /// File a deferred collection block under the CIDs it awaits. It carries
    /// no document and no collection id, so its re-drive is never pushed as a
    /// document write; the collection is resolved from the block itself.
    pub(crate) fn defer_collection_block(
        &self,
        cid: &Cid,
        metadata: &BlockMetadata<'_>,
        awaiting: Vec<WaitKey>,
    ) {
        self.deferred.defer(
            MergeBlock {
                cid: *cid,
                block_data: bytes::Bytes::new(),
                doc_id: String::new(),
                collection_id: String::new(),
                creator: metadata.creator.unwrap_or_default().to_string(),
                sender_peer: metadata.sender_peer.map(str::to_string),
                is_explicit_replicator: metadata.is_explicit_replicator,
                explicit_replay_authorization: metadata.explicit_replay_authorization.clone(),
                verified_creator: None,
            },
            awaiting,
        );
    }
}
