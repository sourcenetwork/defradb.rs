use std::sync::Arc;

use async_trait::async_trait;
use cid::Cid;
use db::merge::governance::{
    MergeCandidate, MergeGovernance, MergeValidator, MergeVerdict, MergeView,
};
use db::merge::merge_handler::DbMergeHandler;
use defra_core::merge::{MergeBlock, MergeOutcome};
use document::{Document, NormalValue};

use super::fixture::{converge, make_handler, merge_turn, read_document, History};

struct AwaitCommittedDocument {
    waiter: Cid,
    dependency: Cid,
    doc_id: String,
}

#[async_trait]
impl MergeValidator for AwaitCommittedDocument {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        if *candidate.cid == self.waiter
            && view
                .immutable_fields("Users", &self.doc_id)
                .await?
                .is_none()
        {
            return Ok(MergeVerdict::defer(
                "document not merged",
                [self.dependency],
            ));
        }
        Ok(MergeVerdict::Accept)
    }
}

#[tokio::test]
async fn committed_history_frames_release_deferred_composites_before_cleanup() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), 1, "revision")
        .await;
    let dependency = history.root().clone();
    history
        .append(handler.blockstore().as_ref(), 63, "revision")
        .await;

    let collection = handler
        .db()
        .find_collection_by_id("col-users")
        .unwrap()
        .unwrap();
    let mut document = Document::new();
    document.set("name", NormalValue::String("waiting".into()));
    let blocks = db::block::builder::build_blocks_from_document(
        &document,
        collection.schema(),
        handler.blockstore(),
    )
    .await
    .unwrap();
    let waiter = MergeBlock {
        cid: blocks.cid,
        block_data: blocks.block,
        doc_id: blocks.doc_id,
        collection_id: "col-users".into(),
        creator: "history-peer".into(),
        sender_peer: Some("history-peer".into()),
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
        verified_creator: None,
    };
    handler
        .db()
        .set_merge_governance(MergeGovernance::new(["Users"]).with_validator(Arc::new(
            AwaitCommittedDocument {
                waiter: waiter.cid,
                dependency: dependency.cid,
                doc_id: dependency.doc_id,
            },
        )));
    assert_eq!(
        merge_turn(&handler, &waiter, false).await,
        MergeOutcome::retryable_skip("document not merged")
    );
    assert_eq!(handler.deferred_composites(), 1);

    let root = history.root();
    let mut released = false;
    for _ in 0..128 {
        let outcome = merge_turn(&handler, root, false).await;
        if handler.deferred_composites() == 0 {
            assert_eq!(outcome, MergeOutcome::Yielded);
            let document = read_document(&handler, &waiter).await.unwrap();
            assert_eq!(
                document.get("name"),
                Some(&NormalValue::String("waiting".into()))
            );
            released = true;
            break;
        }
    }
    assert!(released, "committed ancestors must release their waiters");
    converge(&handler, root, true).await;
    assert_eq!(
        read_document(&handler, root).await.unwrap().get("score"),
        Some(&NormalValue::Int(64))
    );
}
