//! NAC writes that decide in the write they make: each try runs on a manager
//! over a fresh write scope, through the store's one retry policy, so a grant,
//! a revocation or a status change that commits first fails the commit and
//! the decision runs again on what committed.

use std::future::Future;
use std::sync::Arc;

use acp::nac::{NacStatus, NodePermission};
use acp::ZanzibarStore;
#[cfg(not(target_arch = "wasm32"))]
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

/// A relationship change as asked, parsed but not yet checked: the relation
/// and the target are `None` when they named nothing the policy has.
#[derive(Debug, Clone)]
pub struct NacRelationshipRequest {
    pub change: NacChange,
    pub relation: Option<NacRelation>,
    pub target: Option<Did>,
}

/// The winning try's value with the status and owner it committed.
#[cfg(not(target_arch = "wasm32"))]
type Committed<T> = (T, NacStatus, Option<Did>);

impl<S> NacManager<S>
where
    S: ZanzibarStore + ?Sized + 'static,
{
    /// Runs `op` on a manager over a fresh write scope per try, then reloads
    /// the committed status and owner; nothing after the commit can fail it.
    /// `None` when the store has no scopes of its own and the caller writes
    /// through it directly.
    #[cfg(not(target_arch = "wasm32"))]
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

    /// No wasm store runs NAC's tries (`ZanzibarStore::transact` keeps its
    /// default there), so the caller always writes through its own scope.
    #[cfg(target_arch = "wasm32")]
    pub(super) async fn in_write<T, F, Fut>(&self, _op: F) -> Result<Option<T>>
    where
        T: MaybeSend + 'static,
        F: Fn(Arc<NacManager<dyn ZanzibarStore>>) -> Fut + MaybeSendSync + 'static,
        Fut: Future<Output = Result<T>> + MaybeSend + 'static,
    {
        Ok(None)
    }

    /// Make `request` for `caller`, deciding in the write. Whether the
    /// relationship was there before.
    pub async fn change_relationship_as(
        &self,
        caller: &NacCaller,
        request: &NacRelationshipRequest,
    ) -> Result<bool> {
        let asked = (caller.clone(), request.clone());
        let decided = self
            .in_write(move |manager| {
                let (caller, request) = asked.clone();
                async move { manager.decide_relationship(&caller, &request).await }
            })
            .await?;
        match decided {
            Some(existed) => Ok(existed),
            None => self.decide_relationship(caller, request).await,
        }
    }

    /// Go's checks in Go's order on this manager's store: NAC set up, the
    /// caller's permission for the change, NAC on, an identity acting, the
    /// relation, the target; then the change, which checks that the identity
    /// manages the relation. A removal for a target that is no DID finds
    /// nothing to take.
    async fn decide_relationship(
        &self,
        caller: &NacCaller,
        request: &NacRelationshipRequest,
    ) -> Result<bool> {
        if self.nac.status().await == NacStatus::NotConfigured {
            return Err(Error::NotConfigured);
        }
        let change = request.change;
        self.require(caller, change.permission()).await?;
        if self.nac.status().await != NacStatus::Enabled {
            return Err(Error::NotEnabled);
        }
        let requestor = caller.identity.as_ref().ok_or(Error::RequiresIdentity)?;
        let relation = request.relation.ok_or(Error::RelationNotInResource)?;
        let target = match (&request.target, change) {
            (Some(target), _) => target,
            (None, NacChange::Add) => return Err(Error::InvalidActor),
            (None, NacChange::Remove) => return Ok(false),
        };
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

#[cfg(not(target_arch = "wasm32"))]
fn acp_error(error: impl std::fmt::Display) -> Error {
    Error::Acp(error.to_string())
}
