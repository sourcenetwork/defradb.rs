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
//! after the commit and whenever the node's drainer runs a pass
//! ([`ExternalDocumentAcp::settle_owed_changes_once`]), woken by commits
//! that staged changes ([`ExternalDocumentAcp::staged_changes_committed`]). A relationship change
//! made through the gate outside a transaction goes to the provider at once,
//! after what is owed on its tuple.
//!
//! A registration is sent with what its record holds
//! ([`PendingRegistration::authority`]) and with nothing a process keeps
//! beside its requests (see `registration.rs`). One with nothing usable is
//! parked as waiting for authority, and every failed submission leaves its
//! reason in the record. What is owed is reported, given a fresh credential and abandoned through
//! [`ExternalDocumentAcp::owed_entries`],
//! [`ExternalDocumentAcp::reauthorize`] and
//! [`ExternalDocumentAcp::abandon_change`].

use std::sync::Arc;

use async_trait::async_trait;
use identity::Did;
use storage::corekv::{IterOptions, Reader, Store};
use storage::namespace::{Namespace, NamespacedStore};
use zanzibar::types::Subject;

use crate::dac::DocumentACP;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::outbox::deny::{owed_denial, OwedDenial};
use crate::outbox::{OwedChanges, StagedCount, TransactionOutbox};
use crate::pending::{pending_key, PendingRegistration, PENDING_PREFIX};
use crate::permission::DocumentPermission;

#[cfg(test)]
#[path = "external/deadline_tests.rs"]
mod deadline_tests;
mod drain;
mod owed;
mod registration;
mod registration_batch;
mod replica;
mod settle;
mod voided;

pub use drain::MAX_CHANGES_PER_REQUEST;
pub use settle::{OwedScope, OwedSettlement, MAX_FAILED_SUBMISSIONS};
pub use voided::{OweAgain, VoidedEffect};

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

/// How long an observation waits at the edge for an owed change of its tuple
/// to settle before it refuses: the drainer settles a change moments after
/// its commit, while the provider can be reached.
pub const OBSERVATION_PATIENCE: std::time::Duration = std::time::Duration::from_secs(5);

/// `future`'s output, unless `deadline` has passed first. The deadline is
/// polled first on every turn, so a future that is always ready, such as a
/// stream of notifications, cannot keep a wait going past it.
async fn before<F>(
    mut deadline: std::pin::Pin<&mut workers::Delay>,
    mut future: std::pin::Pin<&mut F>,
) -> Option<F::Output>
where
    F: std::future::Future,
{
    std::future::poll_fn(|cx| {
        if std::future::Future::poll(deadline.as_mut(), cx).is_ready() {
            return std::task::Poll::Ready(None);
        }
        std::future::Future::poll(future.as_mut(), cx).map(Some)
    })
    .await
}

/// The refusal of an observation whose tuple still owes a change.
fn unsettled_relationship() -> Error {
    Error::ProviderUnavailable("relationship change is unsettled".into())
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
    /// Told every time a record of something owed is written or deleted.
    changed: Arc<regolith::sync::Notify>,
    /// Told when a transaction that staged changes has committed: wakes the
    /// node's drainer, or leaves it a permit when it is busy.
    wake: Arc<regolith::sync::Notify>,
    /// Told of every owed change the provider refuses for good.
    refusals: Arc<regolith::sync::OnceCell<Arc<dyn crate::OwedRefusalListener>>>,
    /// The request `provider` serves and carries the credentials of, when
    /// this gate serves one.
    serving: Option<defra_core::RequestContext>,
    /// The provider bound to no request: what a submission that serves no
    /// request is bound from, so it never inherits another's credentials.
    root: Arc<dyn DocumentACP>,
    /// Told when this gate changes an object's relationships.
    watcher: Arc<regolith::sync::OnceCell<Arc<dyn crate::ObjectChangeListener>>>,
    /// How many changes the store owes in the layout before the owed counts
    /// were split, once read. Nothing writes that layout any more, so the
    /// number cannot change while the store is open.
    legacy: Arc<regolith::sync::OnceCell<u64>>,
}

fn storage_error(error: storage::corekv::Error) -> Error {
    Error::Storage(error.to_string())
}

impl<S> ExternalDocumentAcp<S>
where
    S: Store + 'static,
{
    /// Gate `provider` with the registrations owed in `store`'s ACP namespace.
    pub fn new(provider: Arc<dyn DocumentACP>, store: Arc<S>) -> Self {
        Self {
            root: Arc::clone(&provider),
            provider,
            store: NamespacedStore::new(store, Namespace::Acpstore),
            settling: Arc::default(),
            changed: Arc::new(regolith::sync::Notify::new()),
            wake: Arc::new(regolith::sync::Notify::new()),
            refusals: Arc::default(),
            serving: None,
            watcher: Arc::default(),
            legacy: Arc::default(),
        }
    }

    /// Wait until a transaction that staged changes commits, or return at
    /// once when one did since the last wait: what the node's drainer waits
    /// for between passes.
    pub async fn staged_changes_committed(&self) {
        self.wake.notified().await;
    }

    /// Tell the listener, if any, that the provider refused `change` for
    /// good, and whether it stays parked.
    pub(crate) fn told_refused(
        &self,
        change: &crate::outbox::OwedChange,
        reason: &str,
        kept: bool,
    ) {
        if let Some(listener) = self.refusals.get() {
            listener.refused(change, reason, kept);
        }
    }

    /// Tell the watcher, if any, that this gate changed an object's
    /// relationships.
    pub(crate) fn object_changed(&self, policy_id: &str, resource_name: &str, object_id: &str) {
        if let Some(watcher) = self.watcher.get() {
            watcher.changed(policy_id, resource_name, object_id);
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
        self.owed_layout().await?;
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

    /// Wait until nothing is owed on `tuple`, woken whenever a record of
    /// something owed is written or deleted, until `deadline`. A change at
    /// rest (refused, or waiting for authority) will not settle by itself,
    /// and neither does one the deadline outlasts: both refuse as unsettled.
    async fn await_tuple(
        &self,
        tuple: &crate::outbox::OwedTuple,
        mut deadline: std::pin::Pin<&mut workers::Delay>,
    ) -> Result<()> {
        loop {
            // Asked for before the read: a `Notified` records the count of
            // notifications when it is made, so a record that changes after
            // it, even before its first poll, makes that poll ready.
            let changed = self.changed.notified();
            let transaction = self.store.new_txn(true).await.map_err(storage_error)?;
            let rest = crate::outbox::tuple_rest(&transaction, tuple).await?;
            drop(transaction);
            match rest {
                None => return Ok(()),
                Some(true) => return Err(unsettled_relationship()),
                Some(false) => {}
            }
            // The drainer is not woken from here: a commit already woke it,
            // and waking it again would cut its back-off and spend the
            // change's tries while the provider fails.
            if before(deadline.as_mut(), std::pin::pin!(changed))
                .await
                .is_none()
            {
                return Err(unsettled_relationship());
            }
        }
    }

    /// The provider's answer to `observation`, refused alike when `requester`
    /// may not manage the relation or an owed revocation takes that away.
    async fn observe_as_manager(
        &self,
        observation: crate::ActorRelationshipObservation<'_>,
        requester: &Did,
    ) -> Result<bool> {
        let refused = || Error::NotManager {
            operation: "observe relationship".into(),
        };
        let present = match self.provider.observe_actor_relationship(observation).await {
            Err(Error::NotOwner { .. } | Error::NotManager { .. }) => return Err(refused()),
            other => other?,
        };
        if self
            .revoked(
                &Identity::Authenticated(requester.clone()),
                observation.policy,
                observation.resource,
                observation.document,
            )
            .await?
        {
            return Err(refused());
        }
        Ok(present)
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

    /// Settle every owed registration once, a page at a time. A registration
    /// the provider refuses stays owed, its document still readable only by
    /// its creator, and is counted in [`RedriveReport::failed`].
    pub async fn redrive_owed(&self) -> Result<RedriveReport> {
        self.owed_layout().await?;
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

    /// Refuse while the store owes changes in the layout before the owed
    /// counts were split: this version neither submits them nor denies by
    /// them, so a revocation among them would stop denying unseen. Every
    /// access decision, settling pass and redrive asks first; the store is
    /// read once.
    pub async fn owed_layout(&self) -> Result<()> {
        let count = match self.legacy.get() {
            Some(&count) => count,
            None => {
                let txn = self.store.new_txn(true).await.map_err(storage_error)?;
                let count = crate::outbox::legacy_changes(&txn).await?;
                let _ = self.legacy.set(count);
                count
            }
        };
        if count > 0 {
            return Err(Error::LegacyOwedChanges { count });
        }
        Ok(())
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

    fn proves_decisions(&self) -> bool {
        self.provider.proves_decisions()
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
            changed: Arc::clone(&self.changed),
            wake: Arc::clone(&self.wake),
            refusals: Arc::clone(&self.refusals),
            serving: Some(context.clone()),
            root: Arc::clone(&self.root),
            watcher: Arc::clone(&self.watcher),
            legacy: Arc::clone(&self.legacy),
        }))
    }

    fn stages_relationship_changes(&self) -> bool {
        true
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

    fn outbox_authority_provider(&self) -> Option<Arc<dyn DocumentACP>> {
        Some(Arc::clone(&self.provider))
    }

    fn canonical_actor(&self, did: &Did) -> Did {
        self.provider.canonical_actor(did)
    }

    fn own_actor(&self) -> Option<Did> {
        self.provider.own_actor()
    }

    fn watch_object_changes(&self, listener: Arc<dyn crate::ObjectChangeListener>) {
        let _ = self.watcher.set(listener);
    }

    fn owed_journal(&self) -> Option<Arc<dyn crate::outbox::OwedJournal>> {
        self.root.owed_journal()
    }

    fn watch_owed_refusals(&self, listener: Arc<dyn crate::OwedRefusalListener>) {
        let _ = self.refusals.set(listener);
    }

    async fn replica_owed(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<crate::ReplicaOwed> {
        self.owed_on_replica(policy_id, resource_name, doc_id).await
    }

    async fn stage_replica_unit(
        &self,
        view: &datastore::NamespaceView,
        unit: &crate::ReplicaUnit<'_>,
    ) -> Result<()> {
        self.record_replica_unit(view, unit).await
    }

    async fn adopt_replica(
        &self,
        view: &datastore::NamespaceView,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        creator: &Did,
    ) -> Result<()> {
        self.record_adopted_replica(view, policy_id, resource_name, doc_id, creator)
            .await
    }

    async fn settle_replica(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<()> {
        self.settle_replica_unit(policy_id, resource_name, doc_id)
            .await
    }

    fn submits_subject(&self, subject: &Subject) -> bool {
        self.provider.submits_subject(subject)
    }

    fn owed_authority(&self, actor: &Did) -> Result<crate::outbox::OwedAuthority> {
        self.provider.owed_authority(actor)
    }

    fn command_authority(
        &self,
        actor: &Did,
        command: crate::ProviderCommand<'_>,
    ) -> Result<crate::outbox::OwedAuthority> {
        self.provider.command_authority(actor, command)
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

    async fn check_owed_layout(&self) -> Result<()> {
        self.owed_layout().await
    }

    async fn owed_entries(
        &self,
        filter: crate::outbox::OwedFilter<'_>,
    ) -> Result<Vec<crate::outbox::OwedEntry>> {
        self.entries(filter).await
    }

    async fn await_owed(&self, txn: u64, only: Option<&Did>) -> Result<crate::outbox::OwedWait> {
        self.await_transaction(txn, only).await
    }

    fn owed_changed(&self) {
        self.changed.notify_waiters();
        self.wake.notify_one();
    }

    async fn reauthorize_owed(
        &self,
        actor: &Did,
        entry: &crate::outbox::OwedRef,
    ) -> Result<crate::outbox::OwedOutcome> {
        self.reauthorize(actor, entry).await
    }

    async fn abandon_owed_change(&self, actor: &Did, seq: u64) -> Result<()> {
        self.abandon_change(actor, seq).await
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

    async fn redrive_registrations(&self, objects: &[crate::AcpObject<'_>]) -> Vec<Result<()>> {
        self.settle_all_if_owed(objects).await
    }

    async fn register_objects(
        &self,
        identity: &Did,
        objects: &[crate::AcpObject<'_>],
    ) -> std::result::Result<(), crate::RegistrationFailure> {
        self.provider.register_objects(identity, objects).await
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

    async fn registration_parent(
        &self,
        policy_id: &str,
        child: &str,
        id: &str,
    ) -> Result<crate::RegistrationParent> {
        self.provider
            .registration_parent(policy_id, child, id)
            .await
    }

    async fn register_under_parent(
        &self,
        identity: &Did,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        parent: &crate::RegistrationParent,
    ) -> Result<()> {
        self.provider
            .register_under_parent(identity, policy_id, resource, object_id, parent)
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
        if let Some(owed) = self.owed(policy_id, resource_name, doc_id).await? {
            if owed.parent.is_some() {
                // Returning false could expose this row through a branchable collection.
                return Err(Error::ProviderUnavailable(
                    "parent admission is awaiting confirmation".into(),
                ));
            }
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
            return Ok(owed.parent.is_none().then_some(owed.creator));
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
        self.owed_layout().await?;
        if let Some(owed) = self.owed(policy_id, resource_name, doc_id).await? {
            return Ok(owed.parent.is_none()
                && matches!(identity, Identity::Authenticated(did) if *did == owed.creator));
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
        // Authority comes first: a caller who may not manage the relation gets
        // the one refusal whatever is owed on the document.
        let refused = || Error::NotManager {
            operation: "observe relationship".into(),
        };
        let requester = self.provider.canonical_actor(observation.requester);
        if let Some(pending) = self
            .owed(
                observation.policy,
                observation.resource,
                observation.document,
            )
            .await?
        {
            // Until the provider holds the registration, its creator is the
            // only authority on the document.
            return Err(
                if self.provider.canonical_actor(&pending.creator) == requester {
                    Error::ProviderUnavailable("document registration is unsettled".into())
                } else {
                    refused()
                },
            );
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
        // One deadline for the whole observation: every wait it makes, for
        // a notification or for the settling lock, ends with it.
        let mut deadline = Box::pin(
            workers::delay(OBSERVATION_PATIENCE)
                .map_err(|error| Error::Storage(error.to_string()))?,
        );
        loop {
            // A settlement pass holds this lock while it sends what is owed
            // and clears its records: held from the provider's answer to the
            // check below, an owed change of the tuple cannot be settled in
            // between and so cannot hide that the answer predates it
            // (`proofs/tla/ObservationSettlement.tla`).
            let Some(settling) =
                before(deadline.as_mut(), std::pin::pin!(self.settling.lock())).await
            else {
                // Out of patience before the lock came free: authority still
                // decides first, and the answer itself is not used.
                self.observe_as_manager(observation, &requester).await?;
                return Err(unsettled_relationship());
            };
            let present = self.observe_as_manager(observation, &requester).await?;
            let transaction = self.store.new_txn(true).await.map_err(storage_error)?;
            let rest = crate::outbox::tuple_rest(&transaction, &tuple).await?;
            drop(transaction);
            match rest {
                None => return Ok(present),
                Some(true) => return Err(unsettled_relationship()),
                Some(false) => {}
            }
            drop(settling);
            // The answer predates a change the drainer is about to settle:
            // wait for it here, at the edge, and ask again once it has
            // (`proofs/tla/SettleBeforeObserving.tla`).
            self.await_tuple(&tuple, deadline.as_mut()).await?;
        }
    }

    async fn replica_group_member(
        &self,
        group: &crate::ReplicaGroup,
        signer: &Did,
    ) -> Result<bool> {
        let receiver = self
            .own_actor()
            .ok_or(Error::UnsupportedReplicaMembership)?;
        let _settling = self
            .settling
            .try_lock()
            .ok_or_else(unsettled_relationship)?;
        let member = self.provider.replica_group_member(group, signer).await?;
        let txn = self.store.new_txn(true).await.map_err(storage_error)?;
        if txn
            .get(&pending_key(&group.policy, &group.resource, &group.object))
            .await
            .map_err(storage_error)?
            .is_some()
        {
            return Err(unsettled_relationship());
        }
        for actor in [&receiver, signer] {
            let tuple = crate::outbox::OwedTuple {
                policy_id: group.policy.clone(),
                resource: group.resource.clone(),
                object: group.object.clone(),
                relation: group.relation.clone(),
                subject: crate::outbox::OwedSubject::Actor {
                    did: self.provider.canonical_actor(actor),
                },
            };
            if crate::outbox::tuple_rest(&txn, &tuple).await?.is_some() {
                return Err(unsettled_relationship());
            }
        }
        Ok(member)
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
        let changed = self
            .provider
            .add_actor_relationship(
                requestor,
                target,
                policy_id,
                collection_id,
                doc_id,
                relation,
                managing_relations,
            )
            .await?;
        self.object_changed(policy_id, collection_id, doc_id);
        Ok(changed)
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
        let changed = self
            .provider
            .add_relationship(
                requestor,
                target,
                policy_id,
                collection_id,
                doc_id,
                relation,
                managing_relations,
            )
            .await?;
        self.object_changed(policy_id, collection_id, doc_id);
        Ok(changed)
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
        let changed = self
            .provider
            .delete_actor_relationship(
                requestor,
                target,
                policy_id,
                collection_id,
                doc_id,
                relation,
                managing_relations,
            )
            .await?;
        self.object_changed(policy_id, collection_id, doc_id);
        Ok(changed)
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
        let changed = self
            .provider
            .delete_relationship(
                requestor,
                target,
                policy_id,
                collection_id,
                doc_id,
                relation,
                managing_relations,
            )
            .await?;
        self.object_changed(policy_id, collection_id, doc_id);
        Ok(changed)
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
