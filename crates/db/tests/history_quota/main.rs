//! A legacy quota setting must not strand an otherwise valid history.

#[path = "../merge/long_history/fixture.rs"]
mod fixture;

use fixture::{converge, make_handler, read_document, History};

#[test]
fn legacy_node_quota_does_not_strand_valid_history() {
    workers::block_on(async {
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
        converge(&handler, &root, false).await;
        let document = read_document(&handler, &root).await.unwrap();
        assert_eq!(document.get("score"), Some(&document::NormalValue::Int(64)));

        let txn = handler.db().new_txn(true).await.unwrap();
        assert!(txn
            .systemstore()
            .unwrap()
            .get(format!("/merge-history/v1/admissions/{}", root.cid).as_bytes())
            .await
            .unwrap()
            .is_none());
    })
    .unwrap()
}
