//! DEFRA_MERGE_HISTORY_MAX_NODES in a dedicated process, so the env override
//! cannot race the history walks in the merge test binary.

#[path = "../merge/long_history/fixture.rs"]
mod fixture;

use defra_core::merge::MergeOutcome;
use fixture::{converge, make_handler, merge_turn, read_document, History};

#[tokio::test]
async fn node_quota_suspends_the_root_and_a_cap_raise_resumes_it() {
    std::env::set_var("DEFRA_MERGE_HISTORY_MAX_NODES", "8");

    let initial = make_handler().await;
    let handler = db::merge::merge_handler::DbMergeHandler::new_with_max_merge_depth(
        initial.db().clone(),
        initial.blockstore().clone(),
        8,
    );
    let mut history = History::default();
    history
        .append(handler.blockstore().as_ref(), 64, "revision")
        .await;
    let root = history.root().clone();

    let quota = loop {
        match merge_turn(&handler, &root, false).await {
            MergeOutcome::Yielded => continue,
            MergeOutcome::Skipped { reason, terminal } if !terminal => break reason,
            other => panic!("quota exhaustion must stay retryable, got {other:?}"),
        }
    };
    assert!(quota.contains("DEFRA_MERGE_HISTORY_MAX_NODES"));

    {
        let txn = handler.db().new_txn(true).await.unwrap();
        let store = txn.systemstore().unwrap();
        let roots = store
            .get(b"/merge-history/v1/roots")
            .await
            .unwrap()
            .expect("the suspended root holds its admission slot");
        assert_eq!(serde_json::from_slice::<u64>(&roots).unwrap(), 1);
        assert!(
            store
                .get(format!("/merge-history/v1/{}/state", root.cid).as_bytes())
                .await
                .unwrap()
                .is_some(),
            "the suspended root keeps its traversal state for resumption"
        );
    }

    std::env::set_var("DEFRA_MERGE_HISTORY_MAX_NODES", "262144");
    converge(&handler, &root, false).await;
    let document = read_document(&handler, &root).await.unwrap();
    assert_eq!(document.get("score"), Some(&document::NormalValue::Int(64)));

    {
        let txn = handler.db().new_txn(true).await.unwrap();
        let store = txn.systemstore().unwrap();
        let roots = store
            .get(b"/merge-history/v1/roots")
            .await
            .unwrap()
            .expect("the roots counter survives completion");
        assert_eq!(serde_json::from_slice::<u64>(&roots).unwrap(), 0);
    }
}
