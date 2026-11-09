//! Node access control as the caller `context` names. NAC is not
//! transactional, so a transaction the context carries does not apply.

use std::sync::Arc;

use defra_core::RequestContext;

use crate::ops::request::transaction_error;
use crate::{Node, Result};

impl Node {
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
