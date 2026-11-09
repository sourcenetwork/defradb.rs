//! Node access control operations, Go's `GetNACStatus`, `DisableNAC`,
//! `ReEnableNAC` and the NAC actor relationships: one entry point per
//! operation, which every edge calls. Each checks the caller's node
//! permission as Go's `checkNodeAccess` does.

mod error;

use acp::nac::{NacStatus, NodePermission};
use defra_core::RequestContext;
use identity::Did;
use storage::corekv::Store;

use crate::access::nac::acting_did;
use crate::database::DB;
use crate::error::Error;
use crate::nac::{NacCaller, NacChange, NacRelation, NacRelationshipRequest};

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
        self.change_nac_relationship(context, NacChange::Add, relation, target_actor)
            .await
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
        self.change_nac_relationship(context, NacChange::Remove, relation, target_actor)
            .await
    }

    /// Every way `subject` (a DID) holds each node permission, for a caller
    /// who is `subject`, an admin, or holds `NacStatus`. The caller needs an
    /// identity; a refusal is Go's `not authorized` for `NacStatus`.
    pub async fn explain_nac_subject_as(
        &self,
        context: &RequestContext,
        subject: &str,
    ) -> Result<Vec<acp::nac::PermissionProvenance>, NodeAcpError> {
        let nac = self.nac_manager().ok_or(NodeAcpError::NotConfigured)?;
        if nac.status().await == NacStatus::NotConfigured {
            return Err(NodeAcpError::NotConfigured);
        }
        let requestor = acting_did(context).ok_or(NodeAcpError::RequiresIdentity)?;
        let subject = Did::new(subject).map_err(|_| NodeAcpError::InvalidActor)?;
        if subject.is_wildcard() {
            return Err(NodeAcpError::InvalidActor);
        }
        nac.explain_subject(&requestor, &subject)
            .await
            .map_err(|error| match error {
                crate::nac::Error::PermissionDenied => Error::NotAuthorized {
                    permission: NodePermission::NacStatus.as_str().to_owned(),
                }
                .into(),
                other => other.into(),
            })
    }

    /// Who asks in `context`, with the node's own identity beside it.
    fn nac_caller(&self, context: &RequestContext) -> NacCaller {
        NacCaller {
            identity: acting_did(context),
            node: self.node_did(),
        }
    }

    /// Make `change` of `relation` for `target_actor`. The relation and the
    /// target are only parsed here; every check, in Go's order, runs in the
    /// write that makes the change.
    async fn change_nac_relationship(
        &self,
        context: &RequestContext,
        change: NacChange,
        relation: &str,
        target_actor: &str,
    ) -> Result<bool, NodeAcpError> {
        let nac = self.nac_manager().ok_or(NodeAcpError::NotConfigured)?;
        let request = NacRelationshipRequest {
            change,
            relation: match relation {
                ADMIN_RELATION => Some(NacRelation::Admin),
                other => NodePermission::parse(other).map(NacRelation::Permission),
            },
            target: match target_actor {
                "*" => Some(Did::wildcard()),
                did => Did::new(did).ok(),
            },
        };
        let permission = match change {
            NacChange::Add => NodePermission::NacRelationAdd,
            NacChange::Remove => NodePermission::NacRelationDelete,
        };
        nac.change_relationship_as(&self.nac_caller(context), &request)
            .await
            .map_err(|error| refused(error, permission))
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
