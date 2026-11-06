//! Node access control as the caller `context` names. NAC is not
//! transactional, so a transaction the context carries does not apply.

use std::sync::Arc;

use defra_core::RequestContext;

use crate::ops::request::transaction_error;
use crate::{Node, Result};

impl Node {
    /// Stage one source-specific node permission in the authenticated caller's transaction.
    pub async fn change_nac_source_permission_as(
        &self,
        context: &RequestContext,
        edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        self.runner()
            .change_nac_source_permission_as(context, edge)
            .await
            .map_err(transaction_error)
    }

    /// Stage a source-only change as the configured Node in the caller-owned
    /// transaction. This is a trusted embedded-host capability and is not an
    /// HTTP or GraphQL operation. Native NAC checks the Node on the bound view.
    pub async fn change_host_nac_source_permission_in_txn(
        &self,
        owner: &RequestContext,
        edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        let node = self.node_identity_did().ok_or_else(|| {
            crate::Error::Transaction(query::TransactionError::operation(
                "configured Node identity required",
            ))
        })?;
        let node = identity::Did::new(node).map_err(|error| {
            crate::Error::Transaction(query::TransactionError::operation(error.to_string()))
        })?;
        self.txn_registry
            .change_host_nac_source_permission(owner, node, edge.clone())
            .await
            .map_err(transaction_error)
    }

    /// The node's NAC status. Go's `GetNACStatus`.
    pub async fn nac_status_as(&self, context: &RequestContext) -> Result<acp::nac::NacStatus> {
        let runner = Arc::clone(self.runner());
        let context = context.clone();
        self.on_pool(move || async move {
            runner
                .nac_status_as(&context)
                .await
                .map_err(transaction_error)
        })
        .await
    }

    /// Turn NAC off, keeping its relationships. Go's `DisableNAC`.
    pub async fn disable_nac_as(&self, context: &RequestContext) -> Result<()> {
        let runner = Arc::clone(self.runner());
        let context = context.clone();
        self.on_pool(move || async move {
            runner
                .disable_nac_as(&context)
                .await
                .map_err(transaction_error)
        })
        .await
    }

    /// Turn a disabled NAC back on. Go's `ReEnableNAC`.
    pub async fn re_enable_nac_as(&self, context: &RequestContext) -> Result<()> {
        let runner = Arc::clone(self.runner());
        let context = context.clone();
        self.on_pool(move || async move {
            runner
                .re_enable_nac_as(&context)
                .await
                .map_err(transaction_error)
        })
        .await
    }

    /// Give `target_actor` (a DID, or `*`) `relation` on the node; whether
    /// it existed already. Go's `AddNACActorRelationship`.
    pub async fn add_nac_actor_relationship_as(
        &self,
        context: &RequestContext,
        relation: &str,
        target_actor: &str,
    ) -> Result<bool> {
        let runner = Arc::clone(self.runner());
        let (context, relation, target_actor) = (
            context.clone(),
            relation.to_owned(),
            target_actor.to_owned(),
        );
        self.on_pool(move || async move {
            runner
                .add_nac_actor_relationship_as(&context, &relation, &target_actor)
                .await
                .map_err(transaction_error)
        })
        .await
    }

    /// Take `relation` on the node from `target_actor`; whether it was
    /// there. Go's `DeleteNACActorRelationship`.
    pub async fn delete_nac_actor_relationship_as(
        &self,
        context: &RequestContext,
        relation: &str,
        target_actor: &str,
    ) -> Result<bool> {
        let runner = Arc::clone(self.runner());
        let (context, relation, target_actor) = (
            context.clone(),
            relation.to_owned(),
            target_actor.to_owned(),
        );
        self.on_pool(move || async move {
            runner
                .delete_nac_actor_relationship_as(&context, &relation, &target_actor)
                .await
                .map_err(transaction_error)
        })
        .await
    }
}
