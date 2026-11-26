use super::fixture::{converge, make_handler, merge_turn, read_document, Handler, History};
use db::merge::merge_handler::DbMergeHandler;
use defra_core::merge::{MergeBlock, MergeOutcome};
use document::NormalValue;

struct Bookkeeping {
    finished: bool,
    top: u64,
    accounted: u64,
    stored: u64,
}

async fn bookkeeping(handler: &Handler, root: &cid::Cid) -> Option<Bookkeeping> {
    let txn = handler.db().new_txn(true).await.unwrap();
    let store = txn.systemstore().unwrap();
    let state = store
        .get(format!("/merge-history/v1/{root}/state").as_bytes())
        .await
        .unwrap()?;
    let state: serde_json::Value = serde_json::from_slice(&state).unwrap();
    let mut entries = store
        .iterator(
            storage::corekv::IterOptions::new()
                .with_prefix(format!("/merge-history/v1/{root}/data/").into_bytes()),
        )
        .await
        .unwrap();
    let mut stored = 0;
    while entries.next().await.unwrap().is_some() {
        stored += 1;
    }
    entries.close().await.unwrap();
    Some(Bookkeeping {
        finished: !state["outcome"].is_null(),
        top: state["top"].as_u64().unwrap(),
        accounted: state["keys"].as_u64().unwrap(),
        stored,
    })
}

/// Walks `root` turn by turn and returns the deepest stack it reached.
async fn walk_accounted(handler: &Handler, root: &MergeBlock) -> u64 {
    let mut deepest = 0;
    for _ in 0..1024 {
        let outcome = merge_turn(handler, root, false).await;
        if outcome.is_merged() {
            assert!(bookkeeping(handler, &root.cid).await.is_none());
            return deepest;
        }
        assert_eq!(outcome, MergeOutcome::Yielded);
        let Some(state) = bookkeeping(handler, &root.cid).await else {
            continue;
        };
        assert_eq!(
            state.accounted, state.stored,
            "the root's progress must account every bookkeeping key it holds"
        );
        let open = if state.finished { 0 } else { state.top + 1 };
        assert_eq!(
            state.stored,
            2 * open,
            "only the open stack may hold bookkeeping: a frame and a mark per level"
        );
        deepest = deepest.max(state.top);
    }
    panic!("history made no bounded progress");
}

#[test]
fn large_history_bookkeeping_is_accounted_and_retains_only_the_open_stack() {
    workers::block_on(async {
        let initial = make_handler().await;
        let handler = DbMergeHandler::new_with_max_merge_depth(
            initial.db().clone(),
            initial.blockstore().clone(),
            8,
        );
        let mut history = History::default();
        history
            .append(handler.blockstore().as_ref(), 300, "first")
            .await;
        assert!(
            walk_accounted(&handler, history.root()).await >= 280,
            "the fixture must hold most of the history on the stack at once"
        );

        history
            .append(handler.blockstore().as_ref(), 300, "second")
            .await;
        assert!(
            walk_accounted(&handler, history.root()).await <= 300,
            "a walk stops at the applied history below it"
        );
        converge(&handler, history.root(), false).await;
        assert_eq!(
            read_document(&handler, history.root())
                .await
                .unwrap()
                .get("score"),
            Some(&NormalValue::Int(600))
        );
    })
    .unwrap()
}
