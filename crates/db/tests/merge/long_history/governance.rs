//! Governance must also apply to bounded history turns and their arrivals.

use std::sync::Arc;

use async_trait::async_trait;
use cid::Cid;
use db::merge::governance::{
    MergeCandidate, MergeGovernance, MergeValidator, MergeVerdict, MergeView,
};
use db::merge::merge_handler::DbMergeHandler;
use defra_core::merge::MergeOutcome;
use document::NormalValue;

use super::fixture::{converge, make_handler, merge_turn, read_document, History};

struct NeedsDocument {
    doc: String,
    priority: u64,
    dependency: Cid,
    dependency_doc: String,
}

#[async_trait]
impl MergeValidator for NeedsDocument {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        if candidate.doc_id == self.doc
            && candidate.payload.priority == self.priority
            && view
                .immutable_fields("Users", &self.dependency_doc)
                .await?
                .is_none()
        {
            return Ok(MergeVerdict::defer(
                "dependency not merged",
                [self.dependency],
            ));
        }
        Ok(MergeVerdict::Accept)
    }
}

async fn deferred_history(priority: u64, batch: bool) {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        4,
    );
    let mut dependency = History::default();
    dependency
        .append(handler.blockstore().as_ref(), 1, "dependency")
        .await;
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), 32, "governed")
        .await;
    handler
        .db()
        .set_merge_governance(MergeGovernance::new(["Users"]).with_validator(Arc::new(
            NeedsDocument {
                doc: history.root().doc_id.clone(),
                priority,
                dependency: dependency.root().cid,
                dependency_doc: dependency.root().doc_id.clone(),
            },
        )));

    let mut deferred = false;
    for _ in 0..128 {
        let outcome = merge_turn(&handler, history.root(), batch).await;
        if matches!(
            outcome,
            MergeOutcome::Skipped {
                terminal: false,
                ..
            }
        ) {
            deferred = true;
            break;
        }
        assert!(matches!(outcome, MergeOutcome::Yielded));
    }
    assert!(deferred, "history must stop at the governed frame");
    assert_eq!(handler.deferred_composites(), 1);
    if priority == 32 {
        assert!(read_document(&handler, history.root()).await.is_none());
    } else {
        assert_eq!(
            read_document(&handler, history.root())
                .await
                .unwrap()
                .get("score"),
            Some(&NormalValue::Int((priority - 1) as i64)),
            "the deferred frame and its descendants must not be applied"
        );
    }

    converge(&handler, dependency.root(), batch).await;
    assert_eq!(
        handler.deferred_composites(),
        0,
        "arrival must release the root"
    );
    converge(&handler, history.root(), batch).await;
    assert_eq!(
        read_document(&handler, history.root())
            .await
            .unwrap()
            .get("score"),
        Some(&NormalValue::Int(32))
    );
}

#[tokio::test]
async fn a_deferred_root_blocks_history_writes_and_is_indexed_for_arrival() {
    deferred_history(32, false).await;
    deferred_history(32, true).await;
}

#[tokio::test]
async fn a_deferred_ancestor_stops_the_history_and_indexes_its_root() {
    deferred_history(2, false).await;
    deferred_history(2, true).await;
}

#[tokio::test]
async fn a_history_turn_releases_composites_waiting_on_its_committed_frames() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        4,
    );
    let mut dependency = History::default();
    dependency
        .append(handler.blockstore().as_ref(), 1, "dependency")
        .await;
    let genesis = dependency.root().clone();
    dependency
        .append(handler.blockstore().as_ref(), 31, "dependency")
        .await;
    let mut waiter = History::default();
    waiter
        .append(handler.blockstore().as_ref(), 1, "waiter")
        .await;
    handler
        .db()
        .set_merge_governance(MergeGovernance::new(["Users"]).with_validator(Arc::new(
            NeedsDocument {
                doc: waiter.root().doc_id.clone(),
                priority: 1,
                dependency: genesis.cid,
                dependency_doc: genesis.doc_id,
            },
        )));
    assert!(matches!(
        merge_turn(&handler, waiter.root(), false).await,
        MergeOutcome::Skipped {
            terminal: false,
            ..
        }
    ));
    assert_eq!(handler.deferred_composites(), 1);

    converge(&handler, dependency.root(), true).await;
    assert_eq!(handler.deferred_composites(), 0);
    assert!(
        read_document(&handler, waiter.root()).await.is_some(),
        "the committed history frame must wake the waiter without replaying it"
    );
}
