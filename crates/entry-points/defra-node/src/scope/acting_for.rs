//! A node acting for an embedder-authenticated actor in a node-owned
//! transaction.

use defra_core::TxnId;

use super::Scope;
use crate::{Node, RelationshipChange, Result};

/// The node acting for an actor in a node-owned transaction, from
/// [`crate::TxnNode::acting_for`]: transaction ownership is checked as the
/// node's principal, native NAC and DAC check the actor. It offers only the
/// changes an embedder makes for an actor it has authenticated.
#[derive(Clone)]
pub struct ActingFor<'n> {
    node: &'n Node,
    id: TxnId,
    pub(super) actor: Scope,
}

impl<'n> ActingFor<'n> {
    pub(super) fn new(node: &'n Node, id: TxnId, actor: Scope) -> Self {
        Self { node, id, actor }
    }

    /// This scope presenting the actor's bearer `token` to a remote document
    /// ACP provider instead; none for `None`.
    #[must_use]
    pub fn with_bearer<T>(self, token: T) -> Self
    where
        T: Into<Option<String>>,
    {
        Self {
            actor: self.actor.with_bearer(token.into()),
            ..self
        }
    }

    /// This scope presenting `token` as the actor's delegation bound to the
    /// one command each change sends a remote ACP provider instead.
    #[must_use]
    pub fn with_request_delegation(self, token: String) -> Self {
        Self {
            actor: self.actor.with_request_delegation(token),
            ..self
        }
    }

    /// Add, or `delete`, `relation` on document `doc_id` for `target`, and
    /// say what the change did. On a remote provider the change is staged.
    pub async fn change_dac_actor_relationship(
        &self,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
        delete: bool,
    ) -> Result<RelationshipChange> {
        self.node
            .core()
            .change_dac_actor_relationship_for(
                self.id,
                &self.actor.context(self.node),
                collection,
                doc_id,
                relation,
                target,
                delete,
            )
            .await
    }

    /// Add `relation` on document `doc_id` for `target`; whether it existed
    /// already, as [`RelationshipChange::existed_already`] reads it: on a
    /// remote provider the add is staged and answers `false`.
    pub async fn add_dac_actor_relationship(
        &self,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        self.node
            .core()
            .add_dac_actor_relationship_for(
                self.id,
                &self.actor.context(self.node),
                collection,
                doc_id,
                relation,
                target,
            )
            .await
    }

    /// Take `relation` on document `doc_id` from `target`; whether it was
    /// there, as [`RelationshipChange::record_found`] reads it.
    pub async fn delete_dac_actor_relationship(
        &self,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        self.node
            .core()
            .delete_dac_actor_relationship_for(
                self.id,
                &self.actor.context(self.node),
                collection,
                doc_id,
                relation,
                target,
            )
            .await
    }

    /// Stage a native source grant for the actor. An exactly present add or
    /// exactly absent delete returns `false` without authority; a changing
    /// edge needs the actor's authority.
    pub async fn change_nac_source_permission(
        &self,
        edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        self.node
            .core()
            .change_nac_source_permission_for(self.id, &self.actor.context(self.node), edge)
            .await
    }
}
