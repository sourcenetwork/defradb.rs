use super::QueryRunner;
use crate::executor::QueryExecutor;
use crate::test_utils::{MockFetcher, MockTxnRegistry};

#[test]
fn owner_observation_rejects_a_fetcher_without_a_transaction_view() {
    workers::block_on(async {
        let registry = MockTxnRegistry::new(MockFetcher::new());
        let runner = QueryRunner::with_registry(MockFetcher::new(), Vec::new(), registry);
        let handle = runner.begin_txn(true).await.unwrap();
        let error = runner
            .observe_document_owners_in_txn(
                defra_core::RequestContext::default(),
                &handle,
                "Users",
                &["known".into()],
                None,
            )
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("transaction-bound authorization"),
            "{error}"
        );
        runner.discard_txn(&handle).await.unwrap();
    })
    .unwrap();
}
