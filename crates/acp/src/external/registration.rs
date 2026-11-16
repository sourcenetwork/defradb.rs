//! Settling the registration a create owes an external provider.
//!
//! The registration is sent as its creator with what its record holds: the
//! node's key for that DID, or the delegation the create carried, presented
//! as it was then. The provider it is sent through is asked first, as a
//! create is asked when it is admitted, whether that lets the node send it
//! ([`DocumentACP::command_authority`]). When it does not, a delegation of
//! the creator kept with something else it owes is tried, then the
//! credentials of the request being served. Nothing a process keeps beside
//! its requests is read. A registration nothing lets the node send is not
//! sent: its record says it waits for authority, and every failed
//! submission leaves its reason there.

use std::sync::Arc;

use identity::Did;
use storage::corekv::{Reader, Store, Writer};

use super::{storage_error, ExternalDocumentAcp};
use crate::dac::DocumentACP;
use crate::error::{Error, Result};
use crate::outbox::{OwedAuthority, OwedState};
use crate::pending::PendingRegistration;
use crate::provider_command::ProviderCommand;

impl<S> ExternalDocumentAcp<S>
where
    S: Store,
{
    /// Register the owed creator, then clear the record. A provider that
    /// already holds the object keeps its owner: the record is cleared and the
    /// provider decides from then on. A submission that fails leaves its
    /// reason in the record.
    pub(super) async fn settle(&self, owed: &PendingRegistration) -> Result<()> {
        self.settle_with(owed, None).await
    }

    /// [`Self::settle`], where `also` is a delegation of the creator kept
    /// with something else it owes, tried when the record's own authority
    /// no longer serves.
    pub(super) async fn settle_with(
        &self,
        owed: &PendingRegistration,
        also: Option<&str>,
    ) -> Result<()> {
        let settled = self.register_and_clear(owed, also).await;
        if let Err(error) = &settled {
            self.note_failure(owed, error).await?;
        }
        settled
    }

    async fn register_and_clear(
        &self,
        owed: &PendingRegistration,
        also: Option<&str>,
    ) -> Result<()> {
        let registered = self
            .provider
            .is_doc_registered(&owed.policy_id, &owed.resource_name, &owed.doc_id)
            .await?;
        if !registered {
            match self
                .registrar(owed, also)?
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

    /// The provider serving a request that presents `token` for `actor`,
    /// as a delegation bound to one request when `request_bound`, and
    /// nothing else: naming the presenter keeps a token the process holds
    /// for that DID out of reach.
    pub(super) fn presenting(
        &self,
        actor: &Did,
        token: Option<&str>,
        request_bound: bool,
    ) -> Arc<dyn DocumentACP> {
        let context = defra_core::RequestContext::default();
        let context = match token {
            Some(token) if request_bound => {
                context.with_request_delegation(actor.to_string(), token.to_owned())
            }
            token => context.with_bearer(Some(actor.to_string()), token.map(str::to_owned)),
        };
        self.provider
            .bind_request(&context)
            .unwrap_or_else(|| Arc::clone(&self.provider))
    }

    /// The provider `owed` is registered through: the first that may send
    /// the registration as its creator, asked as a create is asked when it
    /// is admitted ([`DocumentACP::command_authority`]). What the record
    /// holds comes first, then `also`, then the credentials of the request
    /// this gate serves. With none, the refusal of what the record holds.
    fn registrar(
        &self,
        owed: &PendingRegistration,
        also: Option<&str>,
    ) -> Result<Arc<dyn DocumentACP>> {
        let command = ProviderCommand::RegisterObject {
            policy_id: &owed.policy_id,
            resource: &owed.resource_name,
            object: &owed.doc_id,
        };
        let recorded = match &owed.authority {
            Some(OwedAuthority::Delegation { token }) => Some(token.as_str()),
            Some(OwedAuthority::NodeKey) | None => None,
        };
        let by_record = self.presenting(&owed.creator, recorded, owed.request_bound);
        let refusal = match by_record.command_authority(&owed.creator, command) {
            Ok(_) => return Ok(by_record),
            Err(refusal) => refusal,
        };
        if let Some(token) = also {
            let by_other = self.presenting(&owed.creator, Some(token), false);
            if by_other.command_authority(&owed.creator, command).is_ok() {
                return Ok(by_other);
            }
        }
        if self.bound
            && self
                .provider
                .command_authority(&owed.creator, command)
                .is_ok()
        {
            return Ok(Arc::clone(&self.provider));
        }
        Err(refusal)
    }

    /// Keep, with the record of `owed`, why its submission failed: parked as
    /// waiting for authority when nothing lets the node send it, as refused
    /// when the provider said no, and still staged otherwise. An unreachable
    /// provider says nothing about a registration already parked.
    async fn note_failure(&self, owed: &PendingRegistration, error: &Error) -> Result<()> {
        let reason = error.to_string();
        let (state, counted) = match error {
            Error::OwedAuthorityUnavailable { .. } => (Some(OwedState::AwaitingAuthority), false),
            Error::ProviderUnavailable(_) => (None, false),
            Error::NotOwner { .. }
            | Error::NotManager { .. }
            | Error::PermissionDenied(_)
            | Error::InvalidRelation(_) => (
                Some(OwedState::Refused {
                    reason: reason.clone(),
                }),
                true,
            ),
            _ => (Some(OwedState::Staged), true),
        };
        self.rewrite(owed, |record| {
            let state = match state {
                Some(state) => state,
                None if record.state == OwedState::Staged => OwedState::Staged,
                None => return,
            };
            if counted {
                record.attempts = record.attempts.saturating_add(1);
            }
            record.park(state, reason);
        })
        .await?;
        Ok(())
    }

    /// Change the record of `owed` unless it records another create by now.
    /// The record as written, or `None` when it is gone, records another
    /// create, or a transaction that committed meanwhile changed it.
    pub(super) async fn rewrite(
        &self,
        owed: &PendingRegistration,
        change: impl FnOnce(&mut PendingRegistration),
    ) -> Result<Option<PendingRegistration>> {
        let key = owed.key();
        let mut txn = self.store.new_txn(false).await.map_err(storage_error)?;
        let current = txn
            .get(&key)
            .await
            .map_err(storage_error)?
            .map(|bytes| PendingRegistration::decode(&bytes))
            .transpose()?
            .filter(|current| current.same_create(owed));
        let Some(mut record) = current else {
            txn.discard();
            return Ok(None);
        };
        let before = record.clone();
        change(&mut record);
        if record == before {
            txn.discard();
            return Ok(Some(record));
        }
        txn.set(&key, &record.encode()?)
            .await
            .map_err(storage_error)?;
        match txn.commit().await {
            Ok(()) => {
                self.changed.notify_waiters();
                Ok(Some(record))
            }
            Err(error) if error.is_txn_conflict() => Ok(None),
            Err(error) => Err(storage_error(error)),
        }
    }

    /// Clear `owed` unless its record is of another create by now: a record
    /// written again for a re-created document is not this one to clear.
    async fn clear(&self, owed: &PendingRegistration) -> Result<()> {
        let key = owed.key();
        let mut txn = self.store.new_txn(false).await.map_err(storage_error)?;
        let current = txn.get(&key).await.map_err(storage_error)?;
        let unchanged = match current {
            Some(bytes) => PendingRegistration::decode(&bytes)?.same_create(owed),
            None => false,
        };
        if !unchanged {
            txn.discard();
            return Ok(());
        }
        txn.delete(&key).await.map_err(storage_error)?;
        match txn.commit().await {
            Ok(()) => {
                self.changed.notify_waiters();
                Ok(())
            }
            // A transaction that changed the record first owns what it says now.
            Err(error) if error.is_txn_conflict() => Ok(()),
            Err(error) => Err(storage_error(error)),
        }
    }

    pub(super) async fn settle_if_owed(
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
}
