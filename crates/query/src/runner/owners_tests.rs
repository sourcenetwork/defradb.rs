use super::QueryRunner;
use crate::executor::QueryExecutor;
use crate::fetcher::DocFetcher;
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

struct BoundOnlyFetcher {
    inner: MockFetcher,
    bound: std::sync::Arc<dyn acp::DocumentACP>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl crate::fetcher::DocFetcher for BoundOnlyFetcher {
    async fn check_node_permission(
        &self,
        _identity: &identity::Did,
        _permission: acp::nac::NodePermission,
    ) -> crate::Result<Option<bool>> {
        Ok(Some(true))
    }
    async fn transaction_acp(
        &self,
        _provider: std::sync::Arc<dyn acp::DocumentACP>,
    ) -> crate::Result<Option<std::sync::Arc<dyn acp::DocumentACP>>> {
        Ok(Some(self.bound.clone()))
    }
    async fn get_all(&self, collection: &str) -> crate::Result<Vec<document::Document>> {
        self.inner.get_all(collection).await
    }
    async fn stream_by_doc_short_ids(
        &self,
        collection: &str,
        ids: &[u64],
        deleted: bool,
    ) -> crate::Result<Box<dyn crate::doc_stream::DocStream>> {
        self.inner
            .stream_by_doc_short_ids(collection, ids, deleted)
            .await
    }
    async fn stream_all_with_deleted(
        &self,
        collection: &str,
        deleted: bool,
    ) -> crate::Result<Box<dyn crate::doc_stream::DocStream>> {
        self.inner
            .stream_all_with_deleted(collection, deleted)
            .await
    }
    async fn get_by_ids(
        &self,
        collection: &str,
        ids: &[String],
    ) -> crate::Result<crate::fetcher::FetchByIdsResult> {
        self.inner.get_by_ids(collection, ids).await
    }
    async fn get_by_field_value(
        &self,
        collection: &str,
        field: &str,
        value: &str,
    ) -> crate::Result<Vec<document::Document>> {
        self.inner
            .get_by_field_value(collection, field, value)
            .await
    }
}

#[test]
fn delegated_turn_uses_certified_acp_even_if_fetcher_fallback_is_unbound() {
    workers::block_on(async {
        let original: std::sync::Arc<dyn acp::DocumentACP> =
            std::sync::Arc::new(super::NoOpDocumentAcp);
        let bound: std::sync::Arc<dyn acp::DocumentACP> =
            std::sync::Arc::new(super::NoOpDocumentAcp);
        let fetcher = BoundOnlyFetcher {
            inner: MockFetcher::new(),
            bound: bound.clone(),
        };
        let actor =
            identity::Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
        assert_eq!(
            super::turn::turn_node_permission(
                &fetcher,
                &actor,
                acp::nac::NodePermission::DocumentRead,
                false
            )
            .await
            .unwrap(),
            Some(true)
        );
        assert_eq!(
            super::turn::turn_node_permission(
                &fetcher,
                &actor,
                acp::nac::NodePermission::DocumentRead,
                true
            )
            .await
            .unwrap(),
            None
        );
        let certified = fetcher
            .transaction_acp(original.clone())
            .await
            .unwrap()
            .unwrap();
        let fallback = fetcher.scoped_acp(original.clone()).await.unwrap();
        assert!(std::sync::Arc::ptr_eq(&fallback, &original));
        let selected = super::turn::turn_acp(&fetcher, original, Some(certified))
            .await
            .unwrap();
        assert!(std::sync::Arc::ptr_eq(&selected, &bound));
    })
    .unwrap();
}

#[test]
fn unsupported_named_provider_precedes_node_denial() {
    use crate::owners::DocumentAccessDecision;
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
    let output = std::process::Command::new("lake")
        .current_dir(root.join("proofs/lean"))
        .args([
            "env",
            "lean",
            "--run",
            "AccessOracle.lean",
            "named",
            "1",
            "1",
            "1",
            "1",
            "0",
            "0",
            "0",
            "1",
            "1",
            "0",
            "1",
            "0",
            "1",
        ])
        .output()
        .expect("run Lean access owner");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = match String::from_utf8(output.stdout).unwrap().trim() {
        "unavailable" => DocumentAccessDecision::Unavailable,
        other => panic!("unexpected Lean result: {other}"),
    };
    workers::block_on(async {
        let actor =
            identity::Did::new("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap();
        let policy = schema::PolicyDescription::new("policy", "resource");
        let decision = super::owners::named_access_decision(
            &super::NoOpDocumentAcp,
            &actor,
            "read",
            &policy,
            "document",
            Some(false),
        )
        .await
        .unwrap();
        assert_eq!(decision, expected);
    })
    .unwrap();
}
