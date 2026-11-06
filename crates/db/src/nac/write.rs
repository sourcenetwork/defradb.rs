//! NAC writes that decide in the write they make: each try runs on a manager
//! over a fresh write scope, through the store's one retry policy, so a grant,
//! a revocation or a status change that commits first fails the commit and
//! the decision runs again on what committed.

use std::future::Future;
use std::sync::Arc;

use acp::nac::{NacStatus, NodePermission};
use acp::ZanzibarStore;
use acp::{ScopeAttempt, ScopeValue};
use defra_core::thread_bounds::{MaybeSend, MaybeSendSync};
use identity::Did;

use super::{Error, NacCaller, NacManager, Result};

/// What a NAC relationship grants: admin, or one node permission.
#[derive(Debug, Clone, Copy)]
pub enum NacRelation {
    Admin,
    Permission(NodePermission),
}

/// Whether a relationship change adds the relationship or removes it.
#[derive(Debug, Clone, Copy)]
pub enum NacChange {
    Add,
    Remove,
}

impl NacChange {
    /// The node permission the caller needs for this change.
    fn permission(self) -> NodePermission {
        match self {
            NacChange::Add => NodePermission::NacRelationAdd,
            NacChange::Remove => NodePermission::NacRelationDelete,
        }
    }
}

/// The winning try's value with the status and owner it committed.
type Committed<T> = (T, NacStatus, Option<Did>);

impl<S> NacManager<S>
where
    S: ZanzibarStore + ?Sized + 'static,
{
    /// Runs `op` on a manager over a fresh write scope per try, then reloads
    /// the committed status and owner; nothing after the commit can fail it.
    /// `None` when the store has no scopes of its own and the caller writes
    /// through it directly.
    pub(super) async fn in_write<T, F, Fut>(&self, op: F) -> Result<Option<T>>
    where
        T: MaybeSend + 'static,
        F: Fn(Arc<NacManager<dyn ZanzibarStore>>) -> Fut + MaybeSendSync + 'static,
        Fut: Future<Output = Result<T>> + MaybeSend + 'static,
    {
        let nac = Arc::clone(&self.nac);
        let config = self.config.clone();
        let op = Arc::new(op);
        let attempt: ScopeAttempt = Box::new(move |store| {
            let manager = Arc::new(NacManager {
                nac: Arc::new(nac.scoped(store)),
                config: config.clone(),
            });
            let op = Arc::clone(&op);
            Box::pin(async move {
                let tried = async {
                    manager.nac.load().await.map_err(acp_error)?;
                    let value = op(Arc::clone(&manager)).await?;
                    Ok::<_, Error>((value, manager.nac.status().await, manager.nac.owner().await))
                };
                match tried.await {
                    Ok(committed) => Ok(Box::new(committed) as ScopeValue),
                    Err(refusal) => Err(Box::new(refusal) as ScopeValue),
                }
            })
        });
        let outcome = match self.nac.transact(self.config.attempts(), attempt).await {
            Ok(outcome) => outcome,
            Err(error) if error.is_conflict() => return Err(Error::Conflict(error.to_string())),
            Err(error) => return Err(acp_error(error)),
        };
        match outcome {
            None => Ok(None),
            Some(Ok(value)) => {
                let (value, status, owner) = *value
                    .downcast::<Committed<T>>()
                    .map_err(|_| Error::Acp("a NAC write returned an unknown value".into()))?;
                // A reload reads the newest committed state, so a later write
                // that published first is never overwritten; only when it
                // fails does the state this try committed stand in.
                if let Err(error) = self.nac.load().await {
                    tracing::warn!(%error, "NAC reload after a committed write failed");
                    self.nac.publish(status, owner);
                }
                Ok(Some(value))
            }
            Some(Err(refusal)) => Err(*refusal
                .downcast::<Error>()
                .map_err(|_| Error::Acp("a NAC write refused with an unknown error".into()))?),
        }
    }

    /// Add or remove `relation` for `target`, `requestor` asking, deciding in
    /// the write: NAC set up, the caller's permission for the change, NAC on,
    /// then the change itself, which checks that `requestor` manages
    /// `relation`. Whether the relationship was there before.
    pub async fn change_relationship_as(
        &self,
        caller: &NacCaller,
        requestor: &Did,
        change: NacChange,
        relation: NacRelation,
        target: &Did,
    ) -> Result<bool> {
        let asked = (caller.clone(), requestor.clone(), target.clone());
        let decided = self
            .in_write(move |manager| {
                let (caller, requestor, target) = asked.clone();
                async move {
                    manager
                        .decide_relationship(&caller, &requestor, change, relation, &target)
                        .await
                }
            })
            .await?;
        match decided {
            Some(existed) => Ok(existed),
            None => {
                self.decide_relationship(caller, requestor, change, relation, target)
                    .await
            }
        }
    }

    /// The relationship checks in Go's order, then the change, on this
    /// manager's store.
    async fn decide_relationship(
        &self,
        caller: &NacCaller,
        requestor: &Did,
        change: NacChange,
        relation: NacRelation,
        target: &Did,
    ) -> Result<bool> {
        if self.nac.status().await == NacStatus::NotConfigured {
            return Err(Error::NotConfigured);
        }
        self.require(caller, change.permission()).await?;
        if self.nac.status().await != NacStatus::Enabled {
            return Err(Error::NotEnabled);
        }
        match (change, relation) {
            (NacChange::Add, NacRelation::Admin) => self
                .add_admin_direct(requestor, target)
                .await
                .map(|added| !added),
            (NacChange::Add, NacRelation::Permission(permission)) => self
                .add_permission_grant_direct(requestor, target, permission)
                .await
                .map(|added| !added),
            (NacChange::Remove, NacRelation::Admin) => {
                self.remove_admin_direct(requestor, target).await
            }
            (NacChange::Remove, NacRelation::Permission(permission)) => {
                self.remove_permission_grant_direct(requestor, target, permission)
                    .await
            }
        }
    }
}

fn acp_error(error: impl std::fmt::Display) -> Error {
    Error::Acp(error.to_string())
}
