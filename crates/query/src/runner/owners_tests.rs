use super::{NoOpDocumentAcp, QueryRunner};
use crate::executor::QueryExecutor;
use crate::test_utils::{MockFetcher, MockTxnRegistry};
use acp::{DocumentACP, DocumentPermission, Identity};
use async_trait::async_trait;
use identity::Did;
use std::sync::Arc;

struct ClaimedTransactions;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentACP for ClaimedTransactions {
    fn supports_transactions(&self) -> bool {
        true
    }
    async fn register_doc_object(
        &self,
        actor: &Did,
        policy: &str,
        resource: &str,
        doc: &str,
    ) -> acp::Result<()> {
        NoOpDocumentAcp
            .register_doc_object(actor, policy, resource, doc)
            .await
    }
    async fn is_doc_registered(
        &self,
        policy: &str,
        resource: &str,
        doc: &str,
    ) -> acp::Result<bool> {
        NoOpDocumentAcp
            .is_doc_registered(policy, resource, doc)
            .await
    }
    async fn check_doc_access(
        &self,
        actor: &Identity,
        permission: DocumentPermission,
        policy: &str,
        resource: &str,
        doc: &str,
    ) -> acp::Result<bool> {
        NoOpDocumentAcp
            .check_doc_access(actor, permission, policy, resource, doc)
            .await
    }
    async fn add_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        policy: &str,
        collection: &str,
        doc: &str,
        relation: &str,
        managing: &[String],
    ) -> acp::Result<bool> {
        NoOpDocumentAcp
            .add_actor_relationship(
                requestor, target, policy, collection, doc, relation, managing,
            )
            .await
    }
    async fn delete_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        policy: &str,
        collection: &str,
        doc: &str,
        relation: &str,
        managing: &[String],
    ) -> acp::Result<bool> {
        NoOpDocumentAcp
            .delete_actor_relationship(
                requestor, target, policy, collection, doc, relation, managing,
            )
            .await
    }
    async fn unregister_doc_object(
        &self,
        policy: &str,
        resource: &str,
        doc: &str,
    ) -> acp::Result<()> {
        NoOpDocumentAcp
            .unregister_doc_object(policy, resource, doc)
            .await
    }
}

#[test]
fn owner_observation_rejects_a_capability_claim_without_a_bound_transaction_view() {
    workers::block_on(async {
        let registry = MockTxnRegistry::new(MockFetcher::new());
        let runner = QueryRunner::with_registry(MockFetcher::new(), Vec::new(), registry)
            .with_acp(Arc::new(ClaimedTransactions));
        assert!(runner.acp.supports_transactions());
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
        runner.rollback_txn(&handle).await.unwrap();
    })
    .unwrap();
}
