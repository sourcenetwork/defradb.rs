//! Node access control as the caller `context` names. NAC is not
//! transactional, so a transaction the context carries does not apply.

use std::sync::Arc;

use defra_core::RequestContext;

use super::NodeCore;
use crate::ops::transaction_error;
use crate::Result;

impl NodeCore<'_> {
    /// Stage one source-specific node permission in the authenticated caller's transaction.
    pub(crate) async fn change_nac_source_permission(
        &self,
        context: &RequestContext,
        edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        self.node
            .runner()
            .change_nac_source_permission_as(context, edge)
            .await
            .map_err(transaction_error)
    }

    /// Stage a source-only change as the transaction's authenticated actor.
    /// This is a trusted embedded-host capability and is not an HTTP or
    /// GraphQL operation. Native NAC checks that actor on the bound view.
    pub(crate) async fn change_host_nac_source_permission(
        &self,
        owner: &RequestContext,
        edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        let node = self.node.node_identity_did().ok_or_else(|| {
            crate::Error::Transaction(query::TransactionError::operation(
                "configured Node identity required",
            ))
        })?;
        identity::Did::new(node).map_err(|error| {
            crate::Error::Transaction(query::TransactionError::operation(error.to_string()))
        })?;
        self.node
            .txn_registry
            .change_host_nac_source_permission(owner, edge.clone())
            .await
            .map_err(transaction_error)
    }

    /// The node's NAC status. Go's `GetNACStatus`.
    pub(crate) async fn nac_status(&self, context: &RequestContext) -> Result<acp::nac::NacStatus> {
        let runner = Arc::clone(self.node.runner());
        let context = context.clone();
        self.node
            .on_pool(move || async move {
                runner
                    .nac_status_as(&context)
                    .await
                    .map_err(transaction_error)
            })
            .await
    }

    /// Every way `subject` (a DID) holds each node permission, for a caller
    /// who is `subject`, an admin, or holds `NacStatus`: owner, admin, a
    /// direct grant, or the shortest chain of sources.
    pub(crate) async fn explain_nac_subject(
        &self,
        context: &RequestContext,
        subject: &str,
    ) -> Result<Vec<acp::nac::PermissionProvenance>> {
        let runner = Arc::clone(self.node.runner());
        let (context, subject) = (context.clone(), subject.to_owned());
        self.node
            .on_pool(move || async move {
                runner
                    .explain_nac_subject_as(&context, &subject)
                    .await
                    .map_err(transaction_error)
            })
            .await
    }

    /// Turn NAC off, keeping its relationships. Go's `DisableNAC`.
    pub(crate) async fn disable_nac(&self, context: &RequestContext) -> Result<()> {
        let runner = Arc::clone(self.node.runner());
        let context = context.clone();
        self.node
            .on_pool(move || async move {
                runner
                    .disable_nac_as(&context)
                    .await
                    .map_err(transaction_error)
            })
            .await
    }

    /// Turn a disabled NAC back on. Go's `ReEnableNAC`.
    pub(crate) async fn re_enable_nac(&self, context: &RequestContext) -> Result<()> {
        let runner = Arc::clone(self.node.runner());
        let context = context.clone();
        self.node
            .on_pool(move || async move {
                runner
                    .re_enable_nac_as(&context)
                    .await
                    .map_err(transaction_error)
            })
            .await
    }

    /// Give `target_actor` (a DID, or `*`) `relation` on the node; whether
    /// it existed already. Go's `AddNACActorRelationship`.
    pub(crate) async fn add_nac_actor_relationship(
        &self,
        context: &RequestContext,
        relation: &str,
        target_actor: &str,
    ) -> Result<bool> {
        let runner = Arc::clone(self.node.runner());
        let (context, relation, target_actor) = (
            context.clone(),
            relation.to_owned(),
            target_actor.to_owned(),
        );
        self.node
            .on_pool(move || async move {
                runner
                    .add_nac_actor_relationship_as(&context, &relation, &target_actor)
                    .await
                    .map_err(transaction_error)
            })
            .await
    }

    /// Take `relation` on the node from `target_actor`; whether it was
    /// there. Go's `DeleteNACActorRelationship`.
    pub(crate) async fn delete_nac_actor_relationship(
        &self,
        context: &RequestContext,
        relation: &str,
        target_actor: &str,
    ) -> Result<bool> {
        let runner = Arc::clone(self.node.runner());
        let (context, relation, target_actor) = (
            context.clone(),
            relation.to_owned(),
            target_actor.to_owned(),
        );
        self.node
            .on_pool(move || async move {
                runner
                    .delete_nac_actor_relationship_as(&context, &relation, &target_actor)
                    .await
                    .map_err(transaction_error)
            })
            .await
    }
}
