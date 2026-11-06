//! Node access control operations, Go's `GetNACStatus`, `DisableNAC`,
//! `ReEnableNAC` and the NAC actor relationships: one entry point per
//! operation, which every edge calls. Each checks the caller's node
//! permission as Go's `checkNodeAccess` does.

mod error;

use std::sync::Arc;

use acp::nac::{NacStatus, NodePermission};
use defra_core::RequestContext;
use identity::Did;
use storage::corekv::Store;

use crate::access::nac::acting_did;
use crate::database::DB;
use crate::error::Error;
use crate::nac::{NacCaller, NacChange, NacManagerApi, NacRelation};

pub use error::NodeAcpError;

/// The name Go's node policy gives the relation that manages every other.
const ADMIN_RELATION: &str = "admin";

impl<S> DB<S>
where
    S: Store + 'static,
{
    /// The node's NAC status, for a caller with `NacStatus`. Go's
    /// `GetNACStatus`, which answers when NAC was never set up too.
    pub async fn nac_status_as(&self, context: &RequestContext) -> Result<NacStatus, NodeAcpError> {
        self.check_node_access_as(context, NodePermission::NacStatus)
            .await?;
        Ok(match self.nac_manager() {
            Some(nac) => nac.status().await,
            None => NacStatus::NotConfigured,
        })
    }

    /// Turn NAC off, keeping its relationships for a re-enable. Go's
    /// `DisableNAC`: refused when NAC is not set up or already off, then
    /// checked against `NacDisable`, all in the write that turns it off.
    pub async fn disable_nac_as(&self, context: &RequestContext) -> Result<(), NodeAcpError> {
        let nac = self.nac_manager().ok_or(NodeAcpError::NotConfigured)?;
        Ok(nac.disable_as(&self.nac_caller(context)).await?)
    }

    /// Turn a disabled NAC back on with its relationships. Go's
    /// `ReEnableNAC`: refused when NAC is not set up; checked against
    /// `NacReEnable` in the stored relationships even while NAC is off; then
    /// refused when NAC is already on, all in the write that turns it on.
    pub async fn re_enable_nac_as(&self, context: &RequestContext) -> Result<(), NodeAcpError> {
        let nac = self.nac_manager().ok_or(NodeAcpError::NotConfigured)?;
        Ok(nac.re_enable_as(&self.nac_caller(context)).await?)
    }

    /// Give `target_actor` (a DID, or `*` for everyone) `relation` on the
    /// node, the caller asking. Go's `AddNACActorRelationship`; whether the
    /// relationship existed already.
    pub async fn add_nac_actor_relationship_as(
        &self,
        context: &RequestContext,
        relation: &str,
        target_actor: &str,
    ) -> Result<bool, NodeAcpError> {
        let (nac, requestor, relation, target) = self
            .nac_relationship(
                context,
                NodePermission::NacRelationAdd,
                relation,
                target_actor,
            )
            .await?;
        let target = target.ok_or(NodeAcpError::InvalidActor)?;
        nac.change_relationship_as(
            &self.nac_caller(context),
            &requestor,
            NacChange::Add,
            relation,
            &target,
        )
        .await
        .map_err(|error| refused(error, NodePermission::NacRelationAdd))
    }

    /// Take `relation` on the node from `target_actor`, the caller asking.
    /// Go's `DeleteNACActorRelationship`; whether the relationship was
    /// there. A target that is not a DID holds no relationship, so Go's ACP
    /// engine finds none for it rather than refusing it.
    pub async fn delete_nac_actor_relationship_as(
        &self,
        context: &RequestContext,
        relation: &str,
        target_actor: &str,
    ) -> Result<bool, NodeAcpError> {
        let (nac, requestor, relation, target) = self
            .nac_relationship(
                context,
                NodePermission::NacRelationDelete,
                relation,
                target_actor,
            )
            .await?;
        let Some(target) = target else {
            return Ok(false);
        };
        nac.change_relationship_as(
            &self.nac_caller(context),
            &requestor,
            NacChange::Remove,
            relation,
            &target,
        )
        .await
        .map_err(|error| refused(error, NodePermission::NacRelationDelete))
    }

    /// Who asks in `context`, with the node's own identity beside it.
    fn nac_caller(&self, context: &RequestContext) -> NacCaller {
        NacCaller {
            identity: acting_did(context),
            node: self.node_did(),
        }
    }

    /// The node's NAC manager, once NAC was set up.
    async fn configured_nac(&self) -> Result<Arc<dyn NacManagerApi>, NodeAcpError> {
        let nac = self.nac_manager().ok_or(NodeAcpError::NotConfigured)?;
        match nac.status().await {
            NacStatus::NotConfigured => Err(NodeAcpError::NotConfigured),
            _ => Ok(nac),
        }
    }

    /// Everything a relationship operation needs, checked in Go's order:
    /// NAC set up, the caller's `permission`, NAC on, an identity acting,
    /// then the relation; the target, `None` when it is neither a DID nor
    /// `*`, is the operation's to judge. These checks give the refusal Go
    /// gives; the write decides again on what it reads, so one that commits
    /// first cannot slip past them.
    async fn nac_relationship(
        &self,
        context: &RequestContext,
        permission: NodePermission,
        relation: &str,
        target_actor: &str,
    ) -> Result<(Arc<dyn NacManagerApi>, Did, NacRelation, Option<Did>), NodeAcpError> {
        let nac = self.configured_nac().await?;
        self.check_node_access_as(context, permission).await?;
        if nac.status().await != NacStatus::Enabled {
            return Err(NodeAcpError::AcpNotAvailable);
        }
        let requestor = acting_did(context).ok_or(NodeAcpError::RequiresIdentity)?;
        let grant = match relation {
            ADMIN_RELATION => NacRelation::Admin,
            other => NacRelation::Permission(
                NodePermission::parse(other).ok_or(NodeAcpError::RelationNotInResource)?,
            ),
        };
        let target = match target_actor {
            "*" => Some(Did::wildcard()),
            did => Did::new(did).ok(),
        };
        Ok((nac, requestor, grant, target))
    }
}

/// A NAC manager failure, as Go's `not authorized to perform operation`
/// when the manager refused the caller (it manages no such relation).
fn refused(error: crate::nac::Error, permission: NodePermission) -> NodeAcpError {
    match error {
        crate::nac::Error::NotOwner { .. } => Error::NotAuthorized {
            permission: permission.as_str().to_owned(),
        }
        .into(),
        other => other.into(),
    }
}
