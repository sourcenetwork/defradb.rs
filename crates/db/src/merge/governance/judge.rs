use cid::Cid;
use defra_core::block::{Block, CompositeDeltaPayload};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeOutcome};
use schema::CollectionVersion;
use storage::corekv::Store;

use super::validator::MergeCandidate;
use super::view::DbMergeView;
use crate::merge::merge_handler::{DbMergeHandler, MergeError};

pub(crate) enum Judgement {
    Ungoverned,
    Accept,
    Verdict {
        outcome: MergeOutcome,
        awaiting: Vec<Cid>,
    },
}

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    pub(crate) fn is_governed(&self, collection: &CollectionVersion) -> bool {
        self.db
            .merge_governance()
            .is_some_and(|governance| governance.governs(collection))
    }

    /// Judge one composite frame of a governed collection. The signature is
    /// verified here from the block itself, so a frame reached through
    /// recovery, where dispatch skips verification, is judged the same way.
    pub(crate) async fn judge_governed(
        &self,
        cid: &Cid,
        block: &Block,
        payload: &CompositeDeltaPayload,
        doc_id: &str,
        collection: &CollectionVersion,
    ) -> Result<Judgement, MergeError> {
        let Some(governance) = self.db.merge_governance() else {
            return Ok(Judgement::Ungoverned);
        };
        if !governance.governs(collection) {
            return Ok(Judgement::Ungoverned);
        }
        let Some(validator) = governance.validator() else {
            return Ok(Judgement::Verdict {
                outcome: MergeOutcome::retryable_skip(format!(
                    "collection {} is governed but no merge validator is installed",
                    collection.name
                )),
                awaiting: Vec::new(),
            });
        };

        let candidate = MergeCandidate {
            cid,
            block,
            payload,
            doc_id,
            collection,
            is_genesis: block.heads.as_deref().is_none_or(<[Cid]>::is_empty),
            signature: self.frame_signature(cid, block).await?,
        };
        let view = DbMergeView::new(self);
        let verdict = validator.validate(&candidate, &view).await;
        view.finish().await;
        let verdict = verdict.map_err(|error| {
            MergeError::MergeFailed(format!("merge validator failed on {cid}: {error}"))
        })?;
        tracing::debug!(%cid, %doc_id, collection = %collection.name, ?verdict, "Governed composite judged");

        Ok(match verdict.into_outcome() {
            (None, _) => Judgement::Accept,
            (Some(outcome), awaiting) => Judgement::Verdict { outcome, awaiting },
        })
    }

    /// Index the composite being merged under the CIDs a frame of its DAG is
    /// waiting for, keeping only what re-drive needs from the carrier's
    /// metadata.
    pub(crate) fn index_deferred(
        &self,
        root: &Cid,
        doc_id: &str,
        payload: &CompositeDeltaPayload,
        metadata: &BlockMetadata<'_>,
        awaiting: Vec<Cid>,
    ) {
        self.deferred.defer(
            MergeBlock {
                cid: *root,
                block_data: bytes::Bytes::new(),
                doc_id: doc_id.to_string(),
                collection_id: metadata
                    .collection_id
                    .unwrap_or(&payload.schema_version_id)
                    .to_string(),
                creator: metadata.creator.unwrap_or_default().to_string(),
                sender_peer: metadata.sender_peer.map(str::to_string),
                is_explicit_replicator: metadata.is_explicit_replicator,
                explicit_replay_authorization: metadata.explicit_replay_authorization.clone(),
                verified_creator: None,
            },
            awaiting,
        );
    }

    /// Deferred composites currently indexed for re-drive.
    pub fn deferred_composites(&self) -> usize {
        self.deferred.len()
    }
}
