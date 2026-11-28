//! Document relationships as the caller `context` names, in the transaction
//! it carries if it carries one, and as an embedder-authenticated actor in a
//! node-owned transaction.

use std::sync::Arc;

use defra_core::{RequestContext, TxnId};

use super::NodeCore;
use crate::acp_ops::transactional::Relationship;
use crate::{RelationshipChange, Result};

impl NodeCore<'_> {
    /// Add a document relationship as the caller `context` names, in its
    /// transaction when it names one. An anonymous caller is refused by node
    /// and document access control, with Go's errors.
    pub(crate) async fn add_dac_actor_relationship(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        let identity = requester(context);
        let context = context.clone();
        let ops = std::sync::Arc::clone(&self.node.acp_ops);
        let (collection, doc_id, relation, target) = (
            collection.to_owned(),
            doc_id.to_owned(),
            relation.to_owned(),
            target.to_owned(),
        );
        self.node
            .on_acp(move || async move {
                ops.add_dac_actor_relationship(
                    &context,
                    &identity,
                    &collection,
                    &doc_id,
                    &relation,
                    &target,
                )
                .await
            })
            .await
    }

    /// [`Self::add_dac_actor_relationship`] or
    /// [`Self::delete_dac_actor_relationship`], saying what the change
    /// did. In a transaction on a remote provider it is staged; outside one
    /// it is applied before the call returns, whatever the provider.
    pub(crate) async fn change_dac_actor_relationship(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
        delete: bool,
    ) -> Result<RelationshipChange> {
        let identity = requester(context);
        let context = context.clone();
        let ops = std::sync::Arc::clone(&self.node.acp_ops);
        let (collection, doc_id, relation, target) = (
            collection.to_owned(),
            doc_id.to_owned(),
            relation.to_owned(),
            target.to_owned(),
        );
        self.node
            .on_acp(move || async move {
                ops.change_dac_actor_relationship(
                    &context,
                    &identity,
                    crate::acp_ops::transactional::Relationship {
                        collection: &collection,
                        doc_id: &doc_id,
                        relation: &relation,
                        target: &target,
                    },
                    delete,
                )
                .await
            })
            .await
    }

    /// Every relationship on a document, for the authenticated caller when it
    /// owns the document or holds a relation that manages one on its
    /// resource. The same gate serves GET /api/v0/acp/document/relationships.
    pub(crate) async fn list_dac_relationships(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: &str,
    ) -> Result<Vec<acp::DocumentRelationship>> {
        let identity = context
            .identity()
            .ok_or_else(|| query::QueryError::execution("identity required for document ACP"))?
            .to_owned();
        let context = context.clone();
        let ops = std::sync::Arc::clone(&self.node.acp_ops);
        let (collection, doc_id) = (collection.to_owned(), doc_id.to_owned());
        self.node
            .on_acp(move || async move {
                ops.list_dac_relationships(&context, &identity, &collection, &doc_id)
                    .await
            })
            .await
    }

    pub(crate) async fn observe_dac_actor_relationship(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        let identity = context
            .identity()
            .ok_or_else(|| query::QueryError::execution("identity required for document ACP"))?
            .to_owned();
        let context = context.clone();
        let ops = std::sync::Arc::clone(&self.node.acp_ops);
        let (collection, doc_id, relation, target) = (
            collection.to_owned(),
            doc_id.to_owned(),
            relation.to_owned(),
            target.to_owned(),
        );
        self.node
            .on_acp(move || async move {
                ops.observe_dac_actor_relationship(
                    &context,
                    &identity,
                    crate::acp_ops::transactional::Relationship {
                        collection: &collection,
                        doc_id: &doc_id,
                        relation: &relation,
                        target: &target,
                    },
                )
                .await
            })
            .await
    }

    /// Delete a document relationship as the caller `context` names, in its
    /// transaction when it names one. An anonymous caller is refused by node
    /// and document access control, with Go's errors.
    pub(crate) async fn delete_dac_actor_relationship(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        let identity = requester(context);
        let context = context.clone();
        let ops = std::sync::Arc::clone(&self.node.acp_ops);
        let (collection, doc_id, relation, target) = (
            collection.to_owned(),
            doc_id.to_owned(),
            relation.to_owned(),
            target.to_owned(),
        );
        self.node
            .on_acp(move || async move {
                ops.delete_dac_actor_relationship(
                    &context,
                    &identity,
                    &collection,
                    &doc_id,
                    &relation,
                    &target,
                )
                .await
            })
            .await
    }

    /// Change a relationship in the node-owned transaction `id` for an actor
    /// the embedding process has authenticated: transaction ownership is
    /// checked as the node, native NAC and DAC check `actor`. This trusted
    /// embedder boundary must not be exposed as HTTP actor impersonation. On
    /// a remote provider the change is staged.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn change_dac_actor_relationship_for(
        &self,
        id: TxnId,
        actor: &RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
        delete: bool,
    ) -> Result<RelationshipChange> {
        let transaction_context = self.node.node_context().with_txn(Some(id));
        let actor = actor.clone();
        let ops = Arc::clone(&self.node.acp_ops);
        let (collection, doc_id, relation, target) = (
            collection.to_owned(),
            doc_id.to_owned(),
            relation.to_owned(),
            target.to_owned(),
        );
        self.node
            .on_acp(move || async move {
                ops.change_dac_relationship_for_in_txn(
                    &transaction_context,
                    &actor,
                    Relationship {
                        collection: &collection,
                        doc_id: &doc_id,
                        relation: &relation,
                        target: &target,
                    },
                    delete,
                )
                .await
            })
            .await
    }

    /// [`Self::change_dac_actor_relationship_for`] adding the relationship;
    /// whether it existed already, as [`RelationshipChange::existed_already`]
    /// reads it: on a remote provider the add is staged and answers `false`.
    pub(crate) async fn add_dac_actor_relationship_for(
        &self,
        id: TxnId,
        actor: &RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        self.change_dac_actor_relationship_for(
            id, actor, collection, doc_id, relation, target, false,
        )
        .await
        .map(RelationshipChange::existed_already)
    }

    /// [`Self::change_dac_actor_relationship_for`] deleting the relationship;
    /// whether it was there, as [`RelationshipChange::record_found`] reads it.
    pub(crate) async fn delete_dac_actor_relationship_for(
        &self,
        id: TxnId,
        actor: &RequestContext,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        self.change_dac_actor_relationship_for(
            id, actor, collection, doc_id, relation, target, true,
        )
        .await
        .map(RelationshipChange::record_found)
    }

    /// Stage a native source grant in the node-owned transaction `id` for the
    /// embedder-authenticated `actor`; HTTP cannot select this actor. An
    /// exactly present add or exactly absent delete returns `false` without
    /// authority; a changing edge needs the actor's authority.
    pub(crate) async fn change_nac_source_permission_for(
        &self,
        id: TxnId,
        actor: &RequestContext,
        edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        let transaction_context = self.node.node_context().with_txn(Some(id));
        let actor = actor.clone();
        let ops = Arc::clone(&self.node.acp_ops);
        let edge = edge.clone();
        self.node
            .on_acp(move || async move {
                ops.change_nac_source_for_in_txn(&transaction_context, &actor, &edge)
                    .await
            })
            .await
    }
}

/// The requester a relationship change names: the caller's identity, or none
/// for an anonymous caller. Node and document access control refuse an
/// anonymous change themselves, as Go's do, rather than this node refusing it
/// first with an error of its own.
fn requester(context: &RequestContext) -> String {
    context.identity().unwrap_or_default().to_owned()
}
