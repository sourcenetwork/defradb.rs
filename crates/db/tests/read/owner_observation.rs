use acp::{DocumentACP, DocumentPermission, Identity};
use async_trait::async_trait;
use db::{DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB};
use identity::Did;
use query::{QueryExecutor, QueryRunner};
use std::sync::Arc;

struct ClaimedTransactions {
    inner: acp::LocalDocumentACP,
    bindings: std::sync::atomic::AtomicUsize,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentACP for ClaimedTransactions {
    fn supports_transactions(&self) -> bool {
        true
    }
    fn bind_transaction(&self, _view: datastore::NamespaceView) -> Option<Arc<dyn DocumentACP>> {
        self.bindings
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        None
    }
    async fn register_doc_object(
        &self,
        actor: &Did,
        policy: &str,
        resource: &str,
        doc: &str,
    ) -> acp::Result<()> {
        self.inner
            .register_doc_object(actor, policy, resource, doc)
            .await
    }
    async fn is_doc_registered(
        &self,
        policy: &str,
        resource: &str,
        doc: &str,
    ) -> acp::Result<bool> {
        self.inner.is_doc_registered(policy, resource, doc).await
    }
    async fn check_doc_access(
        &self,
        actor: &Identity,
        permission: DocumentPermission,
        policy: &str,
        resource: &str,
        doc: &str,
    ) -> acp::Result<bool> {
        self.inner
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
        self.inner
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
        self.inner
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
        self.inner
            .unregister_doc_object(policy, resource, doc)
            .await
    }
}

#[test]
fn owner_observation_rejects_a_claim_without_a_bound_transaction_view() {
    workers::block_on(async {
        let db = Arc::new(DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
        let provider = Arc::new(ClaimedTransactions {
            inner: acp::LocalDocumentACP::new(Arc::new(acp::MemoryAcpStore::new())),
            bindings: std::sync::atomic::AtomicUsize::new(0),
        });
        assert!(provider.supports_transactions());
        db.set_document_acp(provider.clone());
        let registry = Arc::new(DbTransactionRegistry::new(db.clone()));
        let runner = QueryRunner::with_arc_registry_and_provider(
            LensedAutoCommitFetcher::new(db.clone()),
            DbCollectionProvider::new_arc(db),
            registry,
        )
        .with_acp(provider.clone());
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
        assert_eq!(
            provider.bindings.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the real transaction fetcher must consult the claiming provider"
        );
        runner.discard_txn(&handle).await.unwrap();
    })
    .unwrap();
}
