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
use crate::nac::NacManagerApi;

pub use error::NodeAcpError;

/// The name Go's node policy gives the relation that manages every other.
const ADMIN_RELATION: &str = "admin";

/// What a NAC relationship grants: admin, or one node permission.
enum Grant {
    Admin,
    Permission(NodePermission),
}

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
    /// checked against `NacDisable`.
    pub async fn disable_nac_as(&self, context: &RequestContext) -> Result<(), NodeAcpError> {
        let nac = self.configured_nac().await?;
        if nac.status().await == NacStatus::DisabledTemporarily {
            return Err(NodeAcpError::AlreadyDisabled);
        }
        self.check_node_access_as(context, NodePermission::NacDisable)
            .await?;
        Ok(nac.disable().await?)
    }

    /// Turn a disabled NAC back on with its relationships. Go's
    /// `ReEnableNAC`: refused when NAC is not set up; checked against
    /// `NacReEnable` in the stored relationships even while NAC is off; then
    /// refused when NAC is already on.
    pub async fn re_enable_nac_as(&self, context: &RequestContext) -> Result<(), NodeAcpError> {
        let nac = self.configured_nac().await?;
        self.check_stored_node_access(context, &nac, NodePermission::NacReEnable)
            .await?;
        if nac.status().await == NacStatus::Enabled {
            return Err(NodeAcpError::AlreadyEnabled);
        }
        Ok(nac.re_enable().await?)
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
        let (nac, requestor, grant, target) = self
            .nac_relationship(
                context,
                NodePermission::NacRelationAdd,
                relation,
                target_actor,
            )
            .await?;
        let added = match grant {
            Grant::Admin => nac.add_admin(&requestor, &target).await,
            Grant::Permission(permission) => {
                nac.add_permission_grant(&requestor, &target, permission)
                    .await
            }
        }
        .map_err(|error| refused(error, NodePermission::NacRelationAdd))?;
        Ok(!added)
    }

    /// Take `relation` on the node from `target_actor`, the caller asking.
    /// Go's `DeleteNACActorRelationship`; whether the relationship was
    /// there.
    pub async fn delete_nac_actor_relationship_as(
        &self,
        context: &RequestContext,
        relation: &str,
        target_actor: &str,
    ) -> Result<bool, NodeAcpError> {
        let (nac, requestor, grant, target) = self
            .nac_relationship(
                context,
                NodePermission::NacRelationDelete,
                relation,
                target_actor,
            )
            .await?;
        match grant {
            Grant::Admin => nac.remove_admin(&requestor, &target).await,
            Grant::Permission(permission) => {
                nac.remove_permission_grant(&requestor, &target, permission)
                    .await
            }
        }
        .map_err(|error| refused(error, NodePermission::NacRelationDelete))
    }

    /// The node's NAC manager, once NAC was set up.
    async fn configured_nac(&self) -> Result<Arc<dyn NacManagerApi>, NodeAcpError> {
        let nac = self.nac_manager().ok_or(NodeAcpError::NotConfigured)?;
        match nac.status().await {
            NacStatus::NotConfigured => Err(NodeAcpError::NotConfigured),
            _ => Ok(nac),
        }
    }

    /// [`Self::check_node_access_as`], against the stored relationships when
    /// NAC is off: Go checks re-enabling this way.
    async fn check_stored_node_access(
        &self,
        context: &RequestContext,
        nac: &Arc<dyn NacManagerApi>,
        permission: NodePermission,
    ) -> Result<(), NodeAcpError> {
        if nac.status().await == NacStatus::Enabled {
            return Ok(self.check_node_access_as(context, permission).await?);
        }
        let identity = acting_did(context);
        if identity.is_some() && identity == self.node_did() {
            return Ok(());
        }
        let identity = identity.unwrap_or_else(Did::wildcard);
        match nac
            .check_permission_persisted(&identity, permission)
            .await?
        {
            true => Ok(()),
            false => Err(Error::NotAuthorized {
                permission: permission.as_str().to_owned(),
            }
            .into()),
        }
    }

    /// Everything a relationship operation needs, checked in Go's order:
    /// NAC set up, the caller's `permission`, NAC on, an identity acting,
    /// then the relation and the target.
    async fn nac_relationship(
        &self,
        context: &RequestContext,
        permission: NodePermission,
        relation: &str,
        target_actor: &str,
    ) -> Result<(Arc<dyn NacManagerApi>, Did, Grant, Did), NodeAcpError> {
        let nac = self.configured_nac().await?;
        self.check_node_access_as(context, permission).await?;
        if nac.status().await != NacStatus::Enabled {
            return Err(NodeAcpError::AcpNotAvailable);
        }
        let requestor = acting_did(context).ok_or(NodeAcpError::RequiresIdentity)?;
        let grant = match relation {
            ADMIN_RELATION => Grant::Admin,
            other => Grant::Permission(
                NodePermission::parse(other).ok_or(NodeAcpError::RelationNotInResource)?,
            ),
        };
        let target = match target_actor {
            "*" => Did::wildcard(),
            did => Did::new(did).map_err(|_| NodeAcpError::InvalidActor)?,
        };
        Ok((nac, requestor, grant, target))
    }
}

/// A NAC manager failure, as Go's `not authorized to perform operation`
/// when the manager refused the caller (it manages no such relation).
fn refused(error: crate::nac::Error, permission: NodePermission) -> NodeAcpError {
    let message = error.to_string();
    match acp::normalize_auth_error(message.clone(), permission.as_str()) == message {
        true => error.into(),
        false => Error::NotAuthorized {
            permission: permission.as_str().to_owned(),
        }
        .into(),
    }
}
