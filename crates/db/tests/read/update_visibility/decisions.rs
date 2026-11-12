//! A provider that decides only once it is bound to a transaction, and counts
//! the bindings and the read decisions made through them.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use acp::{DocumentACP, DocumentPermission, Identity};
use async_trait::async_trait;
use identity::Did;

pub(super) struct Decisions {
    inner: Arc<dyn DocumentACP>,
    pub(super) bound: Arc<AtomicUsize>,
    pub(super) checks: Arc<AtomicUsize>,
    decides: bool,
}

impl Decisions {
    pub(super) fn new(inner: Arc<dyn DocumentACP>) -> Self {
        Self {
            inner,
            bound: Arc::default(),
            checks: Arc::default(),
            decides: false,
        }
    }

    fn unbound<T>(&self) -> acp::Result<T> {
        Err(acp::Error::Storage(
            "decided outside the page's transaction".into(),
        ))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocumentACP for Decisions {
    fn supports_transactions(&self) -> bool {
        true
    }

    fn bind_transaction(&self, view: datastore::NamespaceView) -> Option<Arc<dyn DocumentACP>> {
        self.bound.fetch_add(1, Ordering::SeqCst);
        Some(Arc::new(Self {
            inner: self.inner.bind_transaction(view)?,
            bound: self.bound.clone(),
            checks: self.checks.clone(),
            decides: true,
        }))
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
        if !self.decides {
            return self.unbound();
        }
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
        if !self.decides {
            return self.unbound();
        }
        self.checks.fetch_add(1, Ordering::SeqCst);
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
