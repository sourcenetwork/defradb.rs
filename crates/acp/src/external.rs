//! An external ACP provider behind the registrations creates still owe it.
//!
//! Every reader decides through this gate, so a document whose registration is
//! owed (see [`crate::pending`]) is readable only by its creator, whatever the
//! provider says about it. A redrive registers the creator with the provider
//! and only then clears the record, so the document is never public in
//! between, and a record survives a crash or a provider failure until a later
//! redrive settles it. The provider itself stays at the network edge: its own
//! answers are as fresh as its last round trip.

use std::sync::Arc;

use async_trait::async_trait;
use identity::Did;
use storage::corekv::{IterOptions, Reader, Store, Writer};
use storage::namespace::{Namespace, NamespacedStore};
use zanzibar::types::Subject;

use crate::dac::DocumentACP;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::pending::{pending_key, PendingRegistration, PENDING_PREFIX};
use crate::permission::DocumentPermission;

/// Records read into memory per redrive page, bounded by their encoded size.
const REDRIVE_PAGE_BYTES: usize = 256 * 1024;

/// What one redrive pass did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RedriveReport {
    /// Registrations the provider confirmed and whose records were cleared.
    pub settled: u64,
    /// Registrations still owed: the provider refused or could not be reached.
    pub failed: u64,
}

/// An external (non-transactional) provider gated by the owed registrations
/// recorded in `store`.
pub struct ExternalDocumentAcp<S>
where
    S: Store,
{
    provider: Arc<dyn DocumentACP>,
    store: NamespacedStore<S>,
}

fn storage_error(error: storage::corekv::Error) -> Error {
    Error::Storage(error.to_string())
}

impl<S> ExternalDocumentAcp<S>
where
    S: Store,
{
    /// Gate `provider` with the registrations owed in `store`'s ACP namespace.
    pub fn new(provider: Arc<dyn DocumentACP>, store: Arc<S>) -> Self {
        Self {
            provider,
            store: NamespacedStore::new(store, Namespace::Acpstore),
        }
    }

    /// The registration owed for a document, as committed.
    async fn owed(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<Option<PendingRegistration>> {
        let txn = self.store.new_txn(true).await.map_err(storage_error)?;
        txn.get(&pending_key(policy_id, resource_name, doc_id))
            .await
            .map_err(storage_error)?
            .map(|bytes| PendingRegistration::decode(&bytes))
            .transpose()
    }

    /// Register the owed creator, then clear the record. A provider that
    /// already holds the object keeps its owner: the record is cleared and the
    /// provider decides from then on.
    async fn settle(&self, owed: &PendingRegistration) -> Result<()> {
        let registered = self
            .provider
            .is_doc_registered(&owed.policy_id, &owed.resource_name, &owed.doc_id)
            .await?;
        if !registered {
            match self
                .provider
                .register_doc_object(
                    &owed.creator,
                    &owed.policy_id,
                    &owed.resource_name,
                    &owed.doc_id,
                )
                .await
            {
                Ok(()) | Err(Error::DocumentAlreadyRegistered(_)) => {}
                Err(error) => return Err(error),
            }
        }
        self.clear(owed).await
    }

    /// Clear `owed` unless its record changed since it was read: a record
    /// written again for a re-created document is not this one to clear.
    async fn clear(&self, owed: &PendingRegistration) -> Result<()> {
        let key = owed.key();
        let mut txn = self.store.new_txn(false).await.map_err(storage_error)?;
        let current = txn.get(&key).await.map_err(storage_error)?;
        let unchanged = match current {
            Some(bytes) => PendingRegistration::decode(&bytes)? == *owed,
            None => false,
        };
        if !unchanged {
            txn.discard();
            return Ok(());
        }
        txn.delete(&key).await.map_err(storage_error)?;
        match txn.commit().await {
            Ok(()) => Ok(()),
            // A transaction that changed the record first owns what it says now.
            Err(error) if error.is_txn_conflict() => Ok(()),
            Err(error) => Err(storage_error(error)),
        }
    }

    async fn settle_if_owed(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        match self.owed(policy_id, resource_name, doc_id).await? {
            Some(owed) => self.settle(&owed).await,
            None => Ok(()),
        }
    }

    /// Settle every owed registration once, a page at a time. A registration
    /// the provider refuses stays owed, its document still readable only by
    /// its creator, and is counted in [`RedriveReport::failed`].
    pub async fn redrive_owed(&self) -> Result<RedriveReport> {
        let mut report = RedriveReport::default();
        let mut start = PENDING_PREFIX.to_vec();
        loop {
            let page = self.owed_page(&start).await?;
            let Some(last) = page.last() else {
                if report.failed > 0 {
                    tracing::warn!(
                        target: "acp::audit",
                        settled = report.settled,
                        failed = report.failed,
                        "Owed ACP registrations remain; their documents stay readable only by their creators"
                    );
                }
                return Ok(report);
            };
            start = last.key();
            start.push(0);
            for owed in &page {
                match self.settle(owed).await {
                    Ok(()) => report.settled += 1,
                    Err(error) => {
                        report.failed += 1;
                        tracing::debug!(
                            target: "acp::audit",
                            doc_id = %owed.doc_id,
                            error = %error,
                            "Owed ACP registration not settled"
                        );
                    }
                }
            }
        }
    }

    async fn owed_page(&self, start: &[u8]) -> Result<Vec<PendingRegistration>> {
        let txn = self.store.new_txn(true).await.map_err(storage_error)?;
        let mut iter = txn
            .iterator(
                IterOptions::new()
                    .with_prefix(PENDING_PREFIX.to_vec())
                    .with_start(start.to_vec()),
            )
            .await
            .map_err(storage_error)?;
        let mut page = Vec::new();
        let mut bytes = 0;
        while bytes < REDRIVE_PAGE_BYTES {
            let Some(entry) = iter.next().await.map_err(storage_error)? else {
                break;
            };
            bytes += entry.value.len();
            page.push(PendingRegistration::decode(&entry.value)?);
        }
        Ok(page)
    }
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S> DocumentACP for ExternalDocumentAcp<S>
where
    S: Store + 'static,
{
    fn unregistered_documents_are_public(&self) -> bool {
        self.provider.unregistered_documents_are_public()
    }

    fn settles_owed_registrations(&self) -> bool {
        true
    }

    fn bind_request(&self, context: &defra_core::RequestContext) -> Option<Arc<dyn DocumentACP>> {
        let provider = self.provider.bind_request(context)?;
        Some(Arc::new(Self {
            provider,
            store: NamespacedStore::new(Arc::clone(self.store.inner()), self.store.namespace()),
        }))
    }

    fn bearer_token_for(&self, did: &str) -> Option<String> {
        self.provider.bearer_token_for(did)
    }

    fn caller_of_delegation(&self, token: &str) -> Option<std::result::Result<Did, String>> {
        self.provider.caller_of_delegation(token)
    }

    async fn redrive_registration(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        self.settle_if_owed(policy_id, resource_name, doc_id).await
    }

    async fn register_doc_object(
        &self,
        identity: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        self.provider
            .register_doc_object(identity, policy_id, resource_name, doc_id)
            .await
    }

    async fn register_object(
        &self,
        identity: &Did,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
    ) -> Result<()> {
        self.provider
            .register_object(identity, policy_id, resource_name, object_id)
            .await
    }

    async fn is_doc_registered(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        if self.owed(policy_id, resource_name, doc_id).await?.is_some() {
            return Ok(true);
        }
        self.provider
            .is_doc_registered(policy_id, resource_name, doc_id)
            .await
    }

    async fn get_doc_owner(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<Option<Did>> {
        if let Some(owed) = self.owed(policy_id, resource_name, doc_id).await? {
            return Ok(Some(owed.creator));
        }
        self.provider
            .get_doc_owner(policy_id, resource_name, doc_id)
            .await
    }

    async fn check_doc_access(
        &self,
        identity: &Identity,
        permission: DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        if let Some(owed) = self.owed(policy_id, resource_name, doc_id).await? {
            return Ok(matches!(identity, Identity::Authenticated(did) if *did == owed.creator));
        }
        self.provider
            .check_doc_access(identity, permission, policy_id, resource_name, doc_id)
            .await
    }

    async fn create_access_decision(
        &self,
        identity: &Identity,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
        permission: &str,
    ) -> Result<Option<String>> {
        self.settle_if_owed(policy_id, resource_name, object_id)
            .await?;
        self.provider
            .create_access_decision(identity, policy_id, resource_name, object_id, permission)
            .await
    }

    async fn add_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        policy_id: &str,
        collection_id: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> Result<bool> {
        self.settle_if_owed(policy_id, collection_id, doc_id)
            .await?;
        self.provider
            .add_actor_relationship(
                requestor,
                target,
                policy_id,
                collection_id,
                doc_id,
                relation,
                managing_relations,
            )
            .await
    }

    async fn add_relationship(
        &self,
        requestor: &Did,
        target: Subject,
        policy_id: &str,
        collection_id: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> Result<bool> {
        self.settle_if_owed(policy_id, collection_id, doc_id)
            .await?;
        self.provider
            .add_relationship(
                requestor,
                target,
                policy_id,
                collection_id,
                doc_id,
                relation,
                managing_relations,
            )
            .await
    }

    async fn delete_actor_relationship(
        &self,
        requestor: &Did,
        target: &Did,
        policy_id: &str,
        collection_id: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> Result<bool> {
        self.settle_if_owed(policy_id, collection_id, doc_id)
            .await?;
        self.provider
            .delete_actor_relationship(
                requestor,
                target,
                policy_id,
                collection_id,
                doc_id,
                relation,
                managing_relations,
            )
            .await
    }

    async fn delete_relationship(
        &self,
        requestor: &Did,
        target: Subject,
        policy_id: &str,
        collection_id: &str,
        doc_id: &str,
        relation: &str,
        managing_relations: &[String],
    ) -> Result<bool> {
        self.settle_if_owed(policy_id, collection_id, doc_id)
            .await?;
        self.provider
            .delete_relationship(
                requestor,
                target,
                policy_id,
                collection_id,
                doc_id,
                relation,
                managing_relations,
            )
            .await
    }

    async fn unregister_doc_object(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        self.provider
            .unregister_doc_object(policy_id, resource_name, doc_id)
            .await
    }
}
