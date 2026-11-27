//! Changes the provider confirmed on a revision that a later term voided,
//! owed again.
//!
//! A Vera deployment runs on every device of its account, one active at a
//! time. When another device takes over without a handover, the blocks the
//! replaced device finalized above the new term's cut are void: a
//! registration or relationship change the provider confirmed there is gone
//! from the deployment, although its record here was settled and deleted.
//! The provider finds such changes and hands them back
//! ([`OweAgain::owe_again`]); each is recorded again exactly as a create or a
//! transaction records it, so the outbox's rules hold for it: a registration
//! is sent before the changes of its document, the changes of one tuple in
//! order, and an owed delete denies its subject from now until it is settled
//! (`proofs/tla/FollowDeployment.tla`).
//!
//! A change is not owed again when a later one of its tuple, or a
//! registration of its document, was owed already before the call: the
//! later record decides what the provider is sent, and sending the older one
//! after it would undo it. Changes of one tuple handed back together are all
//! owed, in their order.
//!
//! The outbox's bounds on unsettled changes are not applied: they bound what
//! callers stage, while these changes were committed and confirmed already,
//! and the provider admits nothing until they are owed, so a full outbox
//! could never drain (the journal bounds how many one switch hands back).

use async_trait::async_trait;
use identity::Did;
use storage::corekv::Store;

use super::ExternalDocumentAcp;
use super::OwedScope;
use crate::error::{Error, Result};
use crate::outbox::{NewOwedChange, OwedAuthority, OwedTuple, StagedCount};
use crate::pending::{PendingRegistration, PendingRegistrations};

/// A confirmed submission whose effect the deployment no longer holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoidedEffect {
    /// A document's registration, made as `creator`.
    Registration {
        creator: Did,
        policy_id: String,
        resource: String,
        object: String,
        /// The relationships it was registered with.
        relationships: Vec<crate::DerivedRelationship>,
        /// What the registration was sent under.
        authority: OwedAuthority,
        /// Whether that delegation was bound to this one registration.
        request_bound: bool,
    },
    /// A relationship change.
    Change(NewOwedChange),
}

/// Where a provider hands back what a term voided.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait OweAgain: defra_core::thread_bounds::MaybeSendSync {
    /// Record `effects`, in the order the provider confirmed them, as owed
    /// in one transaction. Nothing is recorded when it fails.
    async fn owe_again(&self, effects: &[VoidedEffect]) -> Result<()>;

    /// Submit what is owed, once. Called when the provider takes
    /// submissions again.
    async fn settle_owed(&self);
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S> OweAgain for ExternalDocumentAcp<S>
where
    S: Store + 'static,
{
    async fn owe_again(&self, effects: &[VoidedEffect]) -> Result<()> {
        if effects.is_empty() {
            return Ok(());
        }
        let txn = self.begin(false).await?;
        let view = txn.acpstore();
        view.set_scans_validated(true);
        let registrations = PendingRegistrations::new(&view);
        let changes = self.owed_changes(&view, StagedCount::new());
        let mut staged = 0u64;
        // Tuples this call owes again, and tuples that already owed a later
        // change before it: an effect of the first kind keeps its place
        // behind the one before it, one of the second kind is passed over.
        let mut owing: Vec<&OwedTuple> = Vec::new();
        let mut superseded: Vec<&OwedTuple> = Vec::new();
        for effect in effects {
            match effect {
                VoidedEffect::Registration {
                    creator,
                    policy_id,
                    resource,
                    object,
                    relationships,
                    authority,
                    request_bound,
                } => {
                    if registrations
                        .get(policy_id, resource, object)
                        .await?
                        .is_some()
                    {
                        continue;
                    }
                    // A registration the provider held in a collection that
                    // gates creates passed `create`, which only an arrow over
                    // a derived relationship admits, so it carries some: it
                    // is owed again hidden, as it was first.
                    let pending =
                        PendingRegistration::new(creator.clone(), policy_id, resource, object)
                            .with_relationships(relationships.clone())
                            .gated(!relationships.is_empty())
                            .with_authority(Ok(authority.clone()), *request_bound);
                    registrations.record(&pending).await?;
                }
                VoidedEffect::Change(change) => {
                    let tuple = &change.tuple;
                    if superseded.contains(&tuple) {
                        continue;
                    }
                    if !owing.contains(&tuple) {
                        if changes.tuple(tuple).await?.is_some() {
                            superseded.push(tuple);
                            continue;
                        }
                        owing.push(tuple);
                    }
                    changes.stage_restored(change.clone()).await?;
                }
            }
            staged += 1;
        }
        drop(changes);
        drop(registrations);
        drop(view);
        txn.commit()
            .await
            .map_err(|error| Error::Storage(error.to_string()))?;
        self.changed.notify_waiters();
        // The drainer waits for a commit that staged changes; this is one.
        self.wake.notify_one();
        tracing::warn!(
            target: "acp::audit",
            voided = effects.len(),
            owed = staged,
            "ACP submissions a Vera term voided are owed again"
        );
        Ok(())
    }

    async fn settle_owed(&self) {
        if let Err(error) = self.redrive_owed().await {
            tracing::warn!(%error, "Could not read owed ACP registrations");
        }
        if let Err(error) = self.settle_owed_changes_once(OwedScope::All).await {
            tracing::warn!(%error, "Could not read owed ACP relationship changes");
        }
    }
}

#[cfg(test)]
#[path = "voided_tests.rs"]
mod tests;
