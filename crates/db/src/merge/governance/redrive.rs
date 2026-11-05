use defra_core::merge::{BlockMetadata, MergeOutcome};
use storage::corekv::Store;

use super::deferred::REDRIVE_BUDGET;
use super::redriven::RedrivenMerge;
use crate::merge::merge_handler::DbMergeHandler;

impl<S: Store + 'static, B: blockstore::Blockstore + 'static> DbMergeHandler<S, B> {
    /// Re-merge deferred composites whose awaited CIDs have merged, through
    /// the normal path: signatures are verified and the validator judges each
    /// one again. Bounded by [`REDRIVE_BUDGET`] per call; composites released
    /// by a re-driven merge join the same drain. A composite the database
    /// already holds but no mark covers goes to the sink too: the mark follows
    /// a successful forward, so its forward may still be owed.
    pub async fn redrive_deferred(&self) {
        let mut budget = REDRIVE_BUDGET;
        while budget > 0 {
            let Some(entry) = self.governance.deferred.take_ready() else {
                return;
            };
            budget -= 1;
            let data = match self.blockstore.get(&entry.cid).await {
                Ok(Some(data)) => data,
                Ok(None) => continue,
                Err(error) => {
                    tracing::debug!(cid = %entry.cid, %error, "Deferred composite unreadable for re-drive");
                    continue;
                }
            };
            // A swept composite has no carrier, so it names no creator and no
            // sending peer; an id left empty reads as absent, not as "".
            let mut metadata = BlockMetadata::normal(
                &entry.doc_id,
                &entry.collection_id,
                &entry.creator,
                entry.sender_peer.as_deref(),
                entry.is_explicit_replicator,
            )
            .with_explicit_replay_authorization(entry.explicit_replay_authorization.clone());
            metadata.doc_id = metadata.doc_id.filter(|id| !id.is_empty());
            metadata.collection_id = metadata.collection_id.filter(|id| !id.is_empty());
            metadata.creator = metadata.creator.filter(|creator| !creator.is_empty());
            let outcome = self.merge_with_retries(&entry.cid, &data, metadata).await;
            tracing::debug!(cid = %entry.cid, ?outcome, "Re-drove deferred composite");
            let forward = match &outcome {
                Ok(MergeOutcome::Merged) => true,
                Ok(MergeOutcome::Skipped {
                    reason,
                    terminal: true,
                }) => {
                    MergeOutcome::skip_reason_is_applied(reason)
                        && !matches!(self.blockstore.is_merged(&entry.cid).await, Ok(true))
                }
                _ => false,
            };
            if forward {
                if let Some(sink) = self.redriven_merge_sink() {
                    sink.forward(RedrivenMerge::new(&entry, data)).await;
                }
            }
        }
        if self.governance.deferred.has_ready() {
            tracing::debug!("Re-drive budget spent; remaining composites drain on the next merge");
        }
    }
}
