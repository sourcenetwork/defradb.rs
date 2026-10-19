use super::fixture::{make_handler, merge_turn, Handler, History};
use db::merge::merge_handler::{hook::CompositeMergeHook, DbMergeHandler};
use db::merge::MergeError;
use defra_core::merge::{BlockMetadata, MergeOutcome};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct AdmissionHook(AtomicBool);

#[async_trait::async_trait]
impl CompositeMergeHook for AdmissionHook {
    async fn on_protected_composite(
        &self,
        _: &str,
        _: &schema::CollectionVersion,
        _: &BlockMetadata<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        Ok(
            (!self.0.load(Ordering::SeqCst))
                .then(|| MergeOutcome::retryable_skip("not registered")),
        )
    }
}

async fn admitted_roots(handler: &Handler) -> u64 {
    let txn = handler.db().new_txn(true).await.unwrap();
    let mut entries = txn
        .systemstore()
        .unwrap()
        .iterator(
            storage::corekv::IterOptions::new()
                .with_prefix(b"/merge-history/v1/admissions/".to_vec()),
        )
        .await
        .unwrap();
    let mut count = 0;
    while entries.next().await.unwrap().is_some() {
        count += 1;
    }
    entries.close().await.unwrap();
    count
}

struct ConcurrentAdmission {
    calls: std::sync::atomic::AtomicUsize,
    ready: regolith::sync::Barrier,
}

#[async_trait::async_trait]
impl CompositeMergeHook for ConcurrentAdmission {
    async fn on_protected_composite(
        &self,
        _: &str,
        _: &schema::CollectionVersion,
        _: &BlockMetadata<'_>,
    ) -> Result<Option<MergeOutcome>, MergeError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) < 2 {
            self.ready.wait().await;
        }
        Ok(None)
    }
}

#[tokio::test]
async fn concurrent_admissions_validate_the_last_free_slot() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    for index in 0..63 {
        let mut history = History::default();
        history
            .append(
                handler.blockstore().as_ref(),
                16,
                &format!("occupied-{index}"),
            )
            .await;
        let mut root = history.root().clone();
        root.sender_peer = Some(format!("peer-{}", index / 16));
        assert_eq!(
            merge_turn(&handler, &root, false).await,
            MergeOutcome::Yielded
        );
    }
    let mut left = History::default();
    let mut right = History::default();
    left.append(handler.blockstore().as_ref(), 16, "left").await;
    right
        .append(handler.blockstore().as_ref(), 16, "right")
        .await;
    handler.set_composite_merge_hook(Arc::new(ConcurrentAdmission {
        calls: std::sync::atomic::AtomicUsize::new(0),
        ready: regolith::sync::Barrier::new(2),
    }));
    let (left, right) = tokio::join!(
        merge_turn(&handler, left.root(), false),
        merge_turn(&handler, right.root(), false),
    );
    let outcomes = [left, right];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| **outcome == MergeOutcome::Yielded)
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                MergeOutcome::Skipped {
                    terminal: false,
                    ..
                }
            ))
            .count(),
        1
    );
    assert_eq!(admitted_roots(&handler).await, 64);
}

#[tokio::test]
async fn an_unready_root_is_rejected_before_admission() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    handler.set_composite_merge_hook(Arc::new(AdmissionHook(AtomicBool::new(false))));
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), 32, "unregistered")
        .await;
    for batch in [false, true] {
        assert!(matches!(
            merge_turn(&handler, history.root(), batch).await,
            MergeOutcome::Skipped {
                terminal: false,
                ..
            }
        ));
        assert_eq!(admitted_roots(&handler).await, 0);
        let txn = handler.db().new_txn(true).await.unwrap();
        let mut keys = txn
            .systemstore()
            .unwrap()
            .iterator(
                storage::corekv::IterOptions::new()
                    .with_prefix(format!("/merge-history/v1/{}/", history.root().cid).into_bytes()),
            )
            .await
            .unwrap();
        assert!(keys.next().await.unwrap().is_none());
    }
}

#[tokio::test]
async fn a_sender_cannot_occupy_every_history_slot() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    let mut first = None;
    for index in 0..16 {
        let mut history = History::default();
        history
            .append(handler.blockstore().as_ref(), 32, &format!("noisy-{index}"))
            .await;
        let mut root = history.root().clone();
        root.creator = format!("claimed-creator-{index}");
        assert_eq!(
            merge_turn(&handler, &root, false).await,
            MergeOutcome::Yielded
        );
        first.get_or_insert(root);
    }
    let mut waiting = History::default();
    waiting
        .append(handler.blockstore().as_ref(), 32, "waiting")
        .await;
    let result = merge_turn(&handler, waiting.root(), false).await;
    assert!(matches!(
        result,
        MergeOutcome::Skipped {
            terminal: false,
            ..
        }
    ));
    assert_eq!(admitted_roots(&handler).await, 16);
    let mut other = waiting.root().clone();
    other.sender_peer = Some("other-peer".into());
    assert_eq!(
        merge_turn(&handler, &other, false).await,
        MergeOutcome::Yielded
    );
    assert_eq!(admitted_roots(&handler).await, 17);
    super::fixture::converge(&handler, &first.unwrap(), false).await;
}

#[tokio::test]
async fn loss_of_root_eligibility_releases_existing_admission() {
    let initial = make_handler().await;
    let handler = DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    let hook = Arc::new(AdmissionHook(AtomicBool::new(true)));
    handler.set_composite_merge_hook(hook.clone());
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), 32, "revoked")
        .await;
    assert_eq!(
        merge_turn(&handler, history.root(), false).await,
        MergeOutcome::Yielded
    );
    assert_eq!(admitted_roots(&handler).await, 1);
    hook.0.store(false, Ordering::SeqCst);
    for _ in 0..32 {
        merge_turn(&handler, history.root(), false).await;
        if admitted_roots(&handler).await == 0 {
            return;
        }
    }
    panic!("unready history retained its admission slot");
}
