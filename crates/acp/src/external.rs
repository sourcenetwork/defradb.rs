//! An external ACP provider behind the registrations creates still owe it.
//!
//! Every reader decides through this gate, so a document whose registration is
//! owed (see [`crate::pending`]) is readable only by its creator, whatever the
//! provider says about it. A redrive registers the creator with the provider
//! and only then clears the record, so the document is never public in
//! between, and a record survives a crash or a provider failure until a later
//! redrive settles it. The provider itself stays at the network edge: its own
//! answers are as fresh as its last round trip.
//!
//! The same gate denies whoever an unsettled relationship delete names (see
//! [`crate::outbox`]) before it asks the provider, on every decision it
//! serves: document access, access decisions, and the authority to change a
//! document's relationships. The owner is spared, as the provider names it.
//! Named permission checks use the same gates; relationship listings are not served here.
//!
//! A transaction stages its relationship changes through the gate
//! ([`DocumentACP::transaction_outbox`]), and the gate submits what is owed
//! after the commit and at start
//! ([`ExternalDocumentAcp::settle_owed_changes_once`]). A relationship change
//! made through the gate outside a transaction goes to the provider at once,
//! after what is owed on its tuple.

use std::sync::Arc;

use async_trait::async_trait;
use identity::Did;
use storage::corekv::{IterOptions, Reader, Store, Writer};
use storage::namespace::{Namespace, NamespacedStore};
use zanzibar::types::Subject;

use crate::dac::DocumentACP;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::outbox::deny::{owed_denial, OwedDenial};
use crate::outbox::{OwedChanges, StagedCount, TransactionOutbox};
use crate::pending::{pending_key, PendingRegistration, PENDING_PREFIX};
use crate::permission::DocumentPermission;

mod settle;

pub use settle::{OwedScope, OwedSettlement, MAX_FAILED_SUBMISSIONS};

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
    /// Held by the one pass that submits owed changes: a second pass waits,
    /// so no change is submitted by two at once.
    settling: Arc<regolith::sync::Mutex<()>>,
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
            settling: Arc::default(),
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

    /// The relationship changes owed to the provider, through the transaction
    /// whose ACP namespace `view` is and whose count `staged` is.
    pub fn owed_changes(&self, view: &datastore::NamespaceView, staged: StagedCount) -> OwedChanges
    where
        S: 'static,
    {
        OwedChanges::new(view, Arc::new(self.store.clone()), staged)
    }

    /// Whether a committed, unsettled relationship delete denies `identity`
    /// on an object. The provider is asked for the owner only when one does
    /// and the owner is spared.
    async fn revoked(
        &self,
        identity: &Identity,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
    ) -> Result<bool> {
        let denial = {
            let txn = self.store.new_txn(true).await.map_err(storage_error)?;
            owed_denial(
                &txn,
                &*self.provider,
                identity,
                policy_id,
                resource_name,
                object_id,
            )
            .await?
        };
        match (denial, identity) {
            (OwedDenial::None, _) => Ok(false),
            (OwedDenial::Always, _) | (OwedDenial::UnlessOwner, Identity::Anonymous) => Ok(true),
            (OwedDenial::UnlessOwner, Identity::Authenticated(actor)) => {
                let owner = self
                    .provider
                    .get_doc_owner(policy_id, resource_name, object_id)
                    .await?
                    .map(|owner| self.provider.canonical_actor(&owner));
                Ok(owner != Some(self.provider.canonical_actor(actor)))
            }
        }
    }

    /// Refuse `actor` the authority an owed delete takes from it.
    async fn refuse_revoked(
        &self,
        actor: &Did,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
    ) -> Result<()> {
        let identity = Identity::Authenticated(actor.clone());
        if self
            .revoked(&identity, policy_id, resource_name, object_id)
            .await?
        {
            return Err(Error::PermissionDenied(format!(
                "a revocation owed to the provider denies {actor} on {resource_name} {object_id}"
            )));
        }
        Ok(())
    }

    /// Register the owed creator, then clear the record. A provider that
    /// already holds the object keeps its owner: the record is cleared and the
    /// provider decides from then on.
    async fn settle(&self, owed: &PendingRegistration) -> Result<()> {
        let registered = self
            .provider
            .is_doc_registered(&owed.policy_id, &owed.resource_name, &owed.doc_id)
            .await?;
        self.register_and_clear(&self.provider, owed, registered)
            .await
    }

    /// [`Self::settle`] once the provider said whether it holds the object,
    /// registering through `provider`.
    async fn register_and_clear(
        &self,
        provider: &Arc<dyn DocumentACP>,
        owed: &PendingRegistration,
        registered: bool,
    ) -> Result<()> {
        if !registered {
            match provider
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
            settling: Arc::clone(&self.settling),
        }))
    }

    fn transaction_outbox(
        &self,
        view: &datastore::NamespaceView,
        staged: StagedCount,
    ) -> Option<TransactionOutbox> {
        Some(TransactionOutbox::new(
            Arc::clone(&self.provider),
            view,
            Arc::new(self.store.clone()),
            staged,
        ))
    }

    fn canonical_actor(&self, did: &Did) -> Did {
        self.provider.canonical_actor(did)
    }

    fn submits_subject(&self, subject: &Subject) -> bool {
        self.provider.submits_subject(subject)
    }

    fn owed_authority(&self, actor: &Did) -> Result<crate::outbox::OwedAuthority> {
        self.provider.owed_authority(actor)
    }

    async fn submit_owed_change(
        &self,
        change: &crate::outbox::OwedChange,
    ) -> crate::outbox::Submission {
        self.provider.submit_owed_change(change).await
    }

    async fn settle_owed_changes(&self, scope: OwedScope) -> Result<OwedSettlement> {
        self.settle_owed_changes_once(scope).await
    }

    fn bearer_token_for(&self, did: &str) -> Option<String> {
        self.provider.bearer_token_for(did)
    }

    fn caller_of_delegation(
        &self,
        token: &str,
    ) -> Option<std::result::Result<crate::AuthenticatedCaller, String>> {
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
        if self
            .revoked(identity, policy_id, resource_name, doc_id)
            .await?
        {
            return Ok(false);
        }
        self.provider
            .check_doc_access(identity, permission, policy_id, resource_name, doc_id)
            .await
    }

    async fn observe_actor_relationship(
        &self,
        observation: crate::ActorRelationshipObservation<'_>,
    ) -> Result<bool> {
        self.refuse_revoked(
            observation.requester,
            observation.policy,
            observation.resource,
            observation.document,
        )
        .await?;
        if self
            .owed(
                observation.policy,
                observation.resource,
                observation.document,
            )
            .await?
            .is_some()
        {
            return Err(Error::ProviderUnavailable(
                "document registration is unsettled".into(),
            ));
        }
        let tuple = crate::outbox::OwedTuple {
            policy_id: observation.policy.to_owned(),
            resource: observation.resource.to_owned(),
            object: observation.document.to_owned(),
            relation: observation.relation.to_owned(),
            subject: crate::outbox::OwedSubject::Actor {
                did: self.provider.canonical_actor(observation.target),
            },
        };
        let transaction = self.store.new_txn(true).await.map_err(storage_error)?;
        if crate::outbox::tuple_state(&transaction, &tuple)
            .await?
            .is_some()
        {
            return Err(Error::ProviderUnavailable(
                "relationship change is unsettled".into(),
            ));
        }
        drop(transaction);
        self.provider.observe_actor_relationship(observation).await
    }

    async fn check_named_permission(
        &self,
        subject: &Did,
        permission: &str,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        if subject.is_wildcard() {
            return Err(Error::PermissionDenied(
                "an explicit actor DID is required".into(),
            ));
        }
        if self.owed(policy_id, resource_name, doc_id).await?.is_some()
            || self
                .revoked(
                    &Identity::Authenticated(subject.clone()),
                    policy_id,
                    resource_name,
                    doc_id,
                )
                .await?
        {
            return Ok(false);
        }
        self.provider
            .check_named_permission(subject, permission, policy_id, resource_name, doc_id)
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
        if self
            .revoked(identity, policy_id, resource_name, object_id)
            .await?
        {
            return Err(Error::PermissionDenied(format!(
                "a revocation owed to the provider denies {permission} on {resource_name} {object_id}"
            )));
        }
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
        let subject = Subject::Entity(target.clone());
        self.settle_owed_before(&subject, policy_id, collection_id, doc_id, relation)
            .await?;
        self.refuse_revoked(requestor, policy_id, collection_id, doc_id)
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
        self.settle_owed_before(&target, policy_id, collection_id, doc_id, relation)
            .await?;
        self.refuse_revoked(requestor, policy_id, collection_id, doc_id)
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
        let subject = Subject::Entity(target.clone());
        self.settle_owed_before(&subject, policy_id, collection_id, doc_id, relation)
            .await?;
        self.refuse_revoked(requestor, policy_id, collection_id, doc_id)
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
        self.settle_owed_before(&target, policy_id, collection_id, doc_id, relation)
            .await?;
        self.refuse_revoked(requestor, policy_id, collection_id, doc_id)
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
