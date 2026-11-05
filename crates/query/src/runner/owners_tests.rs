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

#[test]
fn unified_access_rejects_or_reports_unavailable_without_bound_acp() {
    use crate::owners::{DocumentAccessDecision, DocumentAccessMode};
    if !std::process::Command::new("lake")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let oracle = |mode: &str| {
        let output = std::process::Command::new("lake")
            .current_dir(root.join("proofs/lean"))
            .args([
                "env",
                "lean",
                "--run",
                "AccessOracle.lean",
                mode,
                "1",
                "0",
                "1",
                "1",
                "1",
                "1",
                "1",
                "1",
                "1",
                "1",
                "1",
            ])
            .output()
            .expect("run Lean access owner");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let expected_named = match oracle("named").as_str() {
        "unavailable" => DocumentAccessDecision::Unavailable,
        other => panic!("unexpected Lean named result: {other}"),
    };
    let expected_read = oracle("read");
    workers::block_on(async {
        let registry = MockTxnRegistry::new(MockFetcher::new());
        let runner = QueryRunner::with_registry(MockFetcher::new(), Vec::new(), registry);
        let handle = runner.begin_txn(true).await.unwrap();
        let actor =
            identity::Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
        let context = defra_core::RequestContext::default().with_identity(Some(actor.to_string()));
        let ids = vec!["known".into()];
        let named = runner
            .observe_document_access_in_txn(
                context.clone(),
                &handle,
                "Users",
                &ids,
                actor.clone(),
                DocumentAccessMode::Named {
                    permission: "read".into(),
                    node_permission: acp::nac::NodePermission::DocumentRead,
                },
            )
            .await
            .unwrap();
        assert_eq!(named, [expected_named]);
        let delegated = runner
            .observe_document_access_in_txn(
                context,
                &handle,
                "Users",
                &ids,
                actor,
                DocumentAccessMode::DelegatedRead,
            )
            .await;
        match expected_read.as_str() {
            "invalid" => assert!(delegated.is_err()),
            other => panic!("unexpected Lean delegated result: {other}"),
        }
        runner.discard_txn(&handle).await.unwrap();
    })
    .unwrap();
}
