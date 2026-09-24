use cid::Cid;
use defra_core::block::{Block, CrdtDelta};
use defra_core::merge::MergeBlock;
use storage::corekv::Store;

use crate::merge::merge_handler::{DbMergeHandler, MergeError};

/// Most composites enqueued by one sweep pass. The pass that drains them is
/// bounded in turn by [`super::REDRIVE_BUDGET`], so a tick costs at most one
/// enumeration of the unmerged set plus a bounded number of verdicts.
pub const SWEEP_BUDGET: usize = 256;

impl<S: Store + 'static, B: blockstore::Blockstore + 'static> DbMergeHandler<S, B> {
    /// One bounded pass over the composites of governed collections that this
    /// node holds unmerged, re-driving each through the normal verifying merge
    /// path. Returns how many were enqueued.
    ///
    /// This is the fallback for a deferred verdict the re-drive index cannot
    /// hold: a defer naming nothing, a deferral past the index's capacity, a
    /// wait on something that is not a composite, and anything deferred before
    /// a restart, since the index lives in memory. Arrival re-drive remains
    /// the fast path; correctness does not depend on it.
    ///
    /// The unmerged set is the blockstore's own to-merge index, which holds a
    /// key per unmerged block and drops it on `mark_as_merged`, so the cost of
    /// a pass follows the number of unmerged blocks and not the number of
    /// merged ones. A node that has merged everything pays one empty prefix
    /// scan.
    ///
    /// A pass costs nothing until a merge validator is installed. Claiming a
    /// collection by name is not enough: `governs` answers only that the name
    /// was claimed, and an application installing a read or write validator
    /// alone claims names too, so gating on it would have this walk the
    /// unmerged set every interval to re-drive composites no validator can
    /// judge. That is the gate `judge_governed` and `defer_unresolved_document`
    /// already apply.
    pub async fn sweep_unmerged_governed(&self) -> usize {
        let has_validator = self
            .db
            .merge_governance()
            .is_some_and(|governance| governance.validator().is_some());
        if !has_validator {
            return 0;
        }
        let unmerged = match self.blockstore.get_unmerged().await {
            Ok(unmerged) => unmerged,
            Err(error) => {
                tracing::warn!(%error, "Unmerged blocks unreadable for the governance sweep");
                return 0;
            }
        };
        if unmerged.is_empty() {
            return 0;
        }

        let mut enqueued = 0;
        for cid in unmerged {
            if enqueued >= SWEEP_BUDGET {
                tracing::debug!("Governance sweep budget spent; the rest waits for the next tick");
                break;
            }
            match self.sweep_candidate(&cid).await {
                Ok(Some(block)) => {
                    if self.deferred.enqueue_ready(block) {
                        enqueued += 1;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::debug!(%cid, %error, "Unmerged composite not swept")
                }
            }
        }
        if enqueued > 0 {
            self.redrive_deferred().await;
        }
        enqueued
    }

    /// The re-drive entry for an unmerged block, or `None` when the sweep has
    /// no business with it: a block that is not a composite, one whose
    /// collection is not governed, one already merged in this process, or
    /// one a verdict rejected, which no arrival can change.
    async fn sweep_candidate(&self, cid: &Cid) -> Result<Option<MergeBlock>, MergeError> {
        if self.has_merged_composite(cid) || self.rejected_governed.contains_key(cid) {
            return Ok(None);
        }
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
        // A governed definition a verdict deferred is owed a re-judgement
        // like any composite; it is not a document write, so it carries no
        // document and no carrier id.
        if let CrdtDelta::CollectionDefinition(definition) = &block.delta {
            // A version already held was accepted, or defined here; a
            // re-judgement would find it held and do nothing.
            if self
                .db
                .get_collection_by_version_id_full(&cid.to_string())
                .await
                .map_err(MergeError::Database)?
                .is_some()
            {
                return Ok(None);
            }
            // An initial definition carries its root; a patch inherits it
            // from the version it supersedes.
            let governed = match &definition.governance_root {
                Some(_) => true,
                None => self
                    .resolve_previous_collection_version(&block)
                    .await?
                    .is_some_and(|previous| previous.governance_root.is_some()),
            };
            if !governed {
                return Ok(None);
            }
            return Ok(Some(MergeBlock {
                cid: *cid,
                block_data: bytes::Bytes::new(),
                doc_id: String::new(),
                collection_id: String::new(),
                creator: String::new(),
                sender_peer: None,
                is_explicit_replicator: false,
                explicit_replay_authorization: None,
                verified_creator: None,
            }));
        }
        let CrdtDelta::Composite(payload) = &block.delta else {
            return Ok(None);
        };
        // Resolved from the block alone, as a governed collection always is:
        // there is no carrier here to name one.
        let Some(collection) = self
            .block_collection(&payload.schema_version_id, None)
            .await?
            .filter(|collection| self.is_governed(collection.schema()))
        else {
            return Ok(None);
        };

        // A composite whose document cannot be identified is left alone: the
        // merge path would only defer it again on the same missing ancestor,
        // which its own arrival re-drives.
        let doc_id = self.resolve_composite_doc_id(cid, &block, 0).await?;

        Ok(Some(MergeBlock {
            cid: *cid,
            block_data: bytes::Bytes::new(),
            doc_id,
            collection_id: collection.collection_id().to_string(),
            creator: String::new(),
            sender_peer: None,
            is_explicit_replicator: false,
            explicit_replay_authorization: None,
            verified_creator: None,
        }))
    }
}

/// How often the governance sweep runs when nothing else re-drives a
/// deferral.
///
/// The sweep is the slow fallback: arrival re-drive already covers a deferral
/// the index holds, which is the common case, so this only has to bound how
/// long an unreleasable deferral sits. One minute matches the pending-DAG
/// resync it runs beside, and an idle node pays one empty prefix scan for it.
pub const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Run [`DbMergeHandler::sweep_unmerged_governed`] at `interval` until
/// shutdown, starting with one pass now so a restart does not wait a whole
/// interval to re-judge what it deferred before it stopped.
pub async fn run_governance_sweep<S, B>(
    handler: std::sync::Arc<DbMergeHandler<S, B>>,
    interval: std::time::Duration,
    shutdown: p2p::sync::SyncShutdownHandle,
) where
    S: Store + 'static,
    B: blockstore::Blockstore + 'static,
{
    loop {
        if shutdown.is_shutting_down() {
            return;
        }
        let swept = handler.sweep_unmerged_governed().await;
        if swept > 0 {
            tracing::debug!(swept, "Governance sweep re-drove unmerged composites");
        }
        tokio::select! {
            _ = n0_future::time::sleep(interval) => {}
            _ = shutdown.cancelled() => return,
        }
    }
}
