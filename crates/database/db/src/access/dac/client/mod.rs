//! Document actor relationships, Go's `AddDACActorRelationship` and
//! `DeleteDACActorRelationship`, with the policy lookup and relation checks
//! they run: the one implementation HTTP, the CLI, FFI and the embedded node
//! call. A change in a transaction runs in it; an autocommit change runs in a
//! transaction of its own when document ACP joins transactions, and through
//! the provider, at the edge it runs on, when it is a remote one.

mod change;
mod checks;
mod error;
mod in_txn;
mod observe;
mod settle;

use std::sync::Arc;

use defra_core::RequestContext;
use identity::Did;
use storage::corekv::Store;

use crate::DbTransactionRegistry;
use checks::ChangeKind;
use in_txn::NodeAdmin;
use observe::Observation;

pub use change::{DocRelationship, RelationshipChange};
pub use error::DocAcpError;
pub use settle::{settle_owed_changes, OWED_SETTLE_DEADLINE};

type Result<T> = std::result::Result<T, DocAcpError>;

/// The document relationship operations of one node, over its transaction
/// registry and its document ACP provider.
pub struct DocumentRelationships<S>
where
    S: Store + 'static,
{
    registry: Arc<DbTransactionRegistry<S>>,
    provider: Arc<dyn acp::DocumentACP>,
}

impl<S> Clone for DocumentRelationships<S>
where
    S: Store + 'static,
{
    fn clone(&self) -> Self {
        Self {
            registry: Arc::clone(&self.registry),
            provider: Arc::clone(&self.provider),
        }
    }
}

impl<S> DocumentRelationships<S>
where
    S: Store + 'static,
{
    /// The operations over `registry`, deciding through `provider`, the
    /// document ACP the node installed on its database.
    pub fn new(
        registry: Arc<DbTransactionRegistry<S>>,
        provider: Arc<dyn acp::DocumentACP>,
    ) -> Self {
        Self { registry, provider }
    }

    /// Whether a change in a named transaction is made in it: applied there
    /// by a provider that joins the transaction, or staged there as owed to a
    /// remote one.
    pub fn changes_in_transactions(&self) -> bool {
        self.provider.supports_transactions() || self.provider.stages_relationship_changes()
    }

    /// The DID that signed `token`, when the provider takes `token` as a
    /// delegation it may act under now.
    pub fn caller_of_delegation(
        &self,
        token: &str,
    ) -> Option<std::result::Result<acp::AuthenticatedCaller, String>> {
        self.provider.caller_of_delegation(token)
    }

    /// Give `relationship.target` the relation, `identity` asking. Go's
    /// `AddDACActorRelationship`; whether the relationship existed already.
    pub async fn add_as(
        &self,
        context: &RequestContext,
        identity: &str,
        relationship: DocRelationship<'_>,
    ) -> Result<bool> {
        if context.txn().is_some() {
            return self
                .in_txn(
                    context,
                    context,
                    identity,
                    relationship,
                    false,
                    NodeAdmin::Refused,
                )
                .await
                .map(RelationshipChange::existed_already);
        }
        self.apply(context, identity, relationship, ChangeKind::Add)
            .await
    }

    /// Take the relation from `relationship.target`, `identity` asking. Go's
    /// `DeleteDACActorRelationship`; whether the relationship was there.
    pub async fn delete_as(
        &self,
        context: &RequestContext,
        identity: &str,
        relationship: DocRelationship<'_>,
    ) -> Result<bool> {
        if context.txn().is_some() {
            return self
                .in_txn(
                    context,
                    context,
                    identity,
                    relationship,
                    true,
                    NodeAdmin::Refused,
                )
                .await
                .map(RelationshipChange::record_found);
        }
        self.apply(context, identity, relationship, ChangeKind::Delete)
            .await
    }

    /// An add (`delete` false) or a delete, saying whether it was applied or
    /// staged in the transaction `context` names when it names one.
    pub async fn change_as(
        &self,
        context: &RequestContext,
        identity: &str,
        relationship: DocRelationship<'_>,
        delete: bool,
    ) -> Result<RelationshipChange> {
        if context.txn().is_some() {
            return self
                .in_txn(
                    context,
                    context,
                    identity,
                    relationship,
                    delete,
                    NodeAdmin::Refused,
                )
                .await;
        }
        let existed = self
            .apply(context, identity, relationship, ChangeKind::of(delete))
            .await?;
        Ok(RelationshipChange::Applied { existed })
    }

    /// A trusted embedder's change in the transaction
    /// `transaction_context` names, made as the actor `actor_context`
    /// carries. A native node administrator may change a relationship it
    /// does not manage on this route only.
    pub async fn change_for_in_txn(
        &self,
        transaction_context: &RequestContext,
        actor_context: &RequestContext,
        relationship: DocRelationship<'_>,
        delete: bool,
    ) -> Result<RelationshipChange> {
        let identity = actor_context.identity().ok_or_else(|| {
            query::QueryError::permission_denied("an authenticated relationship actor is required")
        })?;
        let actor = Did::new(identity).map_err(DocAcpError::InvalidIdentityDid)?;
        if actor.is_wildcard() {
            return Err(query::QueryError::permission_denied(
                "an explicit relationship actor is required",
            )
            .into());
        }
        self.in_txn(
            transaction_context,
            actor_context,
            identity,
            relationship,
            delete,
            NodeAdmin::Admitted,
        )
        .await
    }

    /// Whether `relationship` holds exactly, as `identity` sees it, in the
    /// transaction `context` names when it names one.
    pub async fn observe_as(
        &self,
        context: &RequestContext,
        identity: &str,
        relationship: DocRelationship<'_>,
    ) -> Result<bool> {
        let requester = Did::new(identity).map_err(DocAcpError::InvalidIdentityDid)?;
        let target = Did::new(relationship.target).map_err(DocAcpError::InvalidIdentityDid)?;
        let (_, binding) = self.collection_policy(relationship.collection)?;
        if context.txn().is_some() {
            return self
                .observe_in_txn(
                    context,
                    Observation {
                        requester,
                        target,
                        policy: binding.id,
                        resource: binding.resource_name,
                        document: relationship.doc_id.to_owned(),
                        relation: relationship.relation.to_owned(),
                    },
                )
                .await;
        }
        Ok(acp::bound_to_request(&self.provider, context)
            .observe_actor_relationship(acp::ActorRelationshipObservation {
                requester: &requester,
                target: &target,
                policy: &binding.id,
                resource: &binding.resource_name,
                document: relationship.doc_id,
                relation: relationship.relation,
            })
            .await?)
    }

    /// Every relationship on a document, for its owner or a requester
    /// holding a relation that manages one on that resource.
    pub async fn list_as(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
    ) -> Result<Vec<acp::DocumentRelationship>> {
        let requester = Did::new(identity).map_err(DocAcpError::InvalidIdentityDid)?;
        let (_, policy) = self.collection_policy(collection)?;
        Ok(acp::bound_to_request(&self.provider, context)
            .list_relationships(&requester, &policy.id, &policy.resource_name, doc_id)
            .await?)
    }

    /// Whether `actor` holds `permission` on a document, for a caller with
    /// `DacStatus`. Reads only.
    pub async fn check_doc_access_as(
        &self,
        context: &RequestContext,
        actor: &Did,
        permission: acp::DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        self.registry
            .db()
            .check_node_access_as(context, acp::nac::NodePermission::DacStatus)
            .await?;
        Ok(acp::bound_to_request(&self.provider, context)
            .check_doc_access(
                &acp::Identity::Authenticated(actor.clone()),
                permission,
                policy_id,
                resource_name,
                doc_id,
            )
            .await?)
    }

    /// An autocommit change: checked in Go's order, then made in a
    /// transaction of its own by a provider that joins transactions, or
    /// through a remote provider directly. Whether the relationship existed
    /// (an add) or was found (a delete).
    async fn apply(
        &self,
        context: &RequestContext,
        identity: &str,
        relationship: DocRelationship<'_>,
        change: ChangeKind,
    ) -> Result<bool> {
        let delete = matches!(change, ChangeKind::Delete);
        if self.provider.supports_transactions() {
            return self
                .autocommit(context, identity, relationship, delete)
                .await;
        }

        let DocRelationship {
            collection,
            doc_id,
            relation,
            target,
        } = relationship;
        let (requestor, resolved) = self
            .requestor(change, identity, collection, [doc_id, relation, target])
            .await?;
        let wrap = change.wrap();

        // Local ACP relationships are node-local (matches Go): a change is
        // not propagated to peers. Cross-node access control is Vera's role.
        let provider = acp::bound_to_request(&self.provider, context);
        if delete {
            return provider
                .delete_relationship(
                    &requestor,
                    resolved.target,
                    &resolved.policy_id,
                    &resolved.resource_name,
                    doc_id,
                    relation,
                    &resolved.managing_relations,
                )
                .await
                .map_err(|error| wrap(error.into()));
        }
        let added = provider
            .add_relationship(
                &requestor,
                resolved.target,
                &resolved.policy_id,
                &resolved.resource_name,
                doc_id,
                relation,
                &resolved.managing_relations,
            )
            .await
            .map_err(|error| wrap(error.into()))?;
        if added {
            settle::publish_document_update(self.registry.db(), &resolved.collection_id, doc_id)
                .await;
        }
        // Go returns ExistedAlready; `added` is its inverse.
        Ok(!added)
    }
}
