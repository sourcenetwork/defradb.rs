//! Type-erased document-ACP relationship operations bridging DB to Node's
//! public DAC API.

use crate::{Error, Result};
use std::sync::Arc;

use defra_core::RequestContext;
use identity::Did;

pub(crate) mod transactional;

#[async_trait::async_trait]
pub(crate) trait AcpOps: Send + Sync {
    async fn change_nac_source_for_in_txn(
        &self,
        _transaction_context: &RequestContext,
        _actor_context: &RequestContext,
        _edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        Err(query::QueryError::execution(
            "transaction-bound node source permissions are unavailable",
        )
        .into())
    }
    /// A trusted embedder selects a document actor while the node owns the
    /// transaction; the HTTP adapter never exposes this operation.
    async fn change_dac_relationship_for_in_txn(
        &self,
        _transaction_context: &RequestContext,
        _actor_context: &RequestContext,
        _relationship: transactional::Relationship<'_>,
        _delete: bool,
    ) -> Result<bool> {
        Err(query::QueryError::execution(
            "transaction-bound relationship delegation is unavailable",
        )
        .into())
    }

    #[cfg(feature = "http")]
    async fn check_doc_access(
        &self,
        context: &RequestContext,
        actor: &Did,
        permission: acp::DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool>;
    async fn add_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool>;
    async fn delete_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool>;
    /// Every relationship on a document, for its owner or a requester holding
    /// a relation that manages one on that resource.
    async fn list_dac_relationships(
        &self,
        _context: &RequestContext,
        _identity: &str,
        _collection: &str,
        _doc_id: &str,
    ) -> Result<Vec<acp::DocumentRelationship>> {
        Err(Error::Acp(acp::Error::UnsupportedRelationshipListing))
    }
}

pub(crate) struct DbAcpOps<S: storage::corekv::Store + 'static> {
    database: Arc<db::DB<S>>,
    registry: Arc<db::DbTransactionRegistry<S>>,
    document_acp: Arc<dyn acp::DocumentACP>,
    event_bus: Arc<dyn events::Bus>,
    executor: Arc<dyn query::QueryExecutor>,
}

/// Everything a relationship call needs from the collection and its policy.
struct RelationshipTarget {
    target: acp::Subject,
    policy_id: String,
    resource_name: String,
    collection_id: String,
    managing_relations: Vec<String>,
}

impl<S: storage::corekv::Store + 'static> DbAcpOps<S> {
    pub(crate) fn new(
        database: Arc<db::DB<S>>,
        registry: Arc<db::DbTransactionRegistry<S>>,
        document_acp: Arc<dyn acp::DocumentACP>,
        event_bus: Arc<dyn events::Bus>,
        executor: Arc<dyn query::QueryExecutor>,
    ) -> Self {
        Self {
            database,
            registry,
            document_acp,
            event_bus,
            executor,
        }
    }

    async fn relationship_autocommit(
        &self,
        context: &RequestContext,
        identity: &str,
        relationship: transactional::Relationship<'_>,
        delete: bool,
    ) -> Result<bool> {
        let guard =
            query::TransactionGuard::begin_as(self.executor.as_ref(), context.clone(), false)
                .await?;
        let id = guard
            .handle()
            .ok_or_else(|| query::QueryError::execution("transaction is unavailable"))?
            .as_str()
            .parse()
            .map_err(|_| query::QueryError::execution("transaction ID must be numeric"))?;
        let txn_context = context.clone().with_txn(Some(id));
        let changed = self
            .relationship_in_txn(
                &txn_context,
                context,
                identity,
                relationship,
                delete,
                transactional::NodeAdmin::Refused,
            )
            .await?;
        guard.commit().await?;
        Ok(changed)
    }

    /// The requestor of a relationship change and what it targets, checked in
    /// the order Go's `AddDACActorRelationship` and
    /// `DeleteDACActorRelationship` check them: the change's node access, that
    /// document ACP is available, then `get-collection` (Go reads the
    /// collection as the caller), the collection name, the collection and its
    /// policy; only then that the identity and every one of `required`
    /// (document, relation, target) are given, and that the relation is not
    /// the owner.
    async fn relationship_requestor(
        &self,
        change: RelationshipChange,
        identity: &str,
        collection: &str,
        required: [&str; 3],
    ) -> Result<(Did, RelationshipTarget)> {
        let did = match identity.is_empty() {
            true => None,
            false => Some(Did::new(identity).map_err(Error::InvalidIdentityDid)?),
        };
        self.database
            .check_node_access(did.as_ref(), change.permission())
            .await?;
        if self.database.dac_policies().is_none() {
            return Err(db::DacPolicyError::AcpNotAvailable.into());
        }
        self.database
            .check_node_access(did.as_ref(), acp::nac::NodePermission::CollectionGet)
            .await?;
        if collection.is_empty() {
            return Err(Error::CollectionNameEmpty);
        }
        let [_, relation, target] = required;
        let wrap = change.wrap();
        let policy = self.collection_policy(collection).await.map_err(wrap)?;
        let did = match did {
            Some(did) if required.iter().all(|value| !value.is_empty()) => did,
            _ => return Err(Error::RelationshipArgumentMissing(change.verb())),
        };
        if relation == acp::OWNER_RELATION {
            return Err(wrap(change.owner_error()));
        }
        let resolved = self
            .relationship_target(policy, relation, target)
            .await
            .map_err(wrap)?;
        Ok((did, resolved))
    }

    /// The policy of collection `collection`, which a relationship needs.
    async fn collection_policy(
        &self,
        collection: &str,
    ) -> Result<(String, schema::PolicyDescription)> {
        let collection_version = self
            .database
            .get_collection(collection)
            .map_err(|error| Error::CollectionLookup {
                collection: collection.to_owned(),
                source: error,
            })?
            .ok_or_else(|| Error::CollectionMissing(collection.to_owned()))?;
        let Some(policy) = collection_version.schema().policy.clone() else {
            return Err(Error::CollectionWithoutPolicy);
        };
        Ok((collection_version.collection_id().to_string(), policy))
    }

    /// The target and the policy facts a relationship change on `relation`
    /// needs, under collection policy `policy`.
    async fn relationship_target(
        &self,
        (collection_id, policy): (String, schema::PolicyDescription),
        relation: &str,
        target: &str,
    ) -> Result<RelationshipTarget> {
        // Parse the target into a structured subject once, at the edge: an actor
        // DID, all-actors `*`, a cross-object edge, or a userset.
        let target = acp::parse_target_subject(target).map_err(Error::InvalidTarget)?;

        let loaded = self
            .database
            .dac_policies()
            .ok_or(db::DacPolicyError::AcpNotAvailable)?
            .get(&policy.id)
            .await?
            .ok_or_else(|| Error::PolicyNotFound(policy.id.clone()))?;
        let resource = loaded
            .get_resource(&policy.resource_name)
            .ok_or_else(|| Error::ResourceNotFound(policy.resource_name.clone()))?;
        if resource.get_relation(relation).is_none() {
            return Err(Error::RelationNotFound {
                relation: relation.to_owned(),
                resource: policy.resource_name.clone(),
            });
        }
        let managing_relations = loaded
            .get_managers_for_relation(&policy.resource_name, relation)
            .into_iter()
            .map(str::to_string)
            .collect();

        Ok(RelationshipTarget {
            target,
            policy_id: policy.id,
            resource_name: policy.resource_name,
            collection_id,
            managing_relations,
        })
    }

    /// Announce the document's latest state so subscribers re-evaluate access.
    ///
    /// Three surfaces treat an unpublishable event differently. Go
    /// (`internal/db/db_dac.go:129-134`, calling `publishDocUpdateEvent` in
    /// `internal/db/db.go`) makes a doc that is absent from the store a silent
    /// no-op and propagates only genuine storage errors. The FFI propagates
    /// both (`crates/ffi/src/acp/dac.rs:401-409`). defra-node deliberately
    /// warns on both and continues: the relationship is already persisted by
    /// the time this runs and the event is only a best-effort notification, so
    /// surfacing an error here would tell the caller the grant failed when it
    /// did not.
    async fn publish_document_update(&self, collection_id: &str, doc_id: &str) {
        match db::block::reader::read_latest_composite_block(&self.database, doc_id).await {
            Ok(result) => {
                self.event_bus
                    .publish(events::Message::update(events::Update::new(
                        doc_id.to_string(),
                        result.cid,
                        collection_id.to_string(),
                        result.block,
                        false,
                        false,
                    )));
            }
            Err(error) => tracing::warn!(
                doc_id,
                error,
                "failed to publish document update after DAC grant"
            ),
        }
    }
}

/// Whether a relationship change adds or deletes.
#[derive(Clone, Copy)]
enum RelationshipChange {
    Add,
    Delete,
}

impl RelationshipChange {
    fn verb(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Delete => "delete",
        }
    }

    fn permission(self) -> acp::nac::NodePermission {
        match self {
            Self::Add => acp::nac::NodePermission::DacRelationAdd,
            Self::Delete => acp::nac::NodePermission::DacRelationDelete,
        }
    }

    /// Go's error for changing the owner relation.
    fn owner_error(self) -> Error {
        match self {
            Self::Add => Error::OwnerRelationAdd,
            Self::Delete => Error::OwnerRelationDelete,
        }
    }

    /// Wraps an error from the policy side of the change in Go's
    /// `failed to <verb> document actor relationship with acp`.
    fn wrap(self) -> impl Fn(Error) -> Error + Copy {
        // What the collection lookup finds before ACP is asked is Go's own
        // error, unwrapped.
        move |source| match source {
            Error::CollectionWithoutPolicy
            | Error::CollectionMissing(_)
            | Error::CollectionLookup { .. } => source,
            source => Error::RelationshipAcp {
                verb: self.verb(),
                source: Box::new(source),
            },
        }
    }
}

#[async_trait::async_trait]
impl<S: storage::corekv::Store + 'static> AcpOps for DbAcpOps<S> {
    async fn list_dac_relationships(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
    ) -> Result<Vec<acp::DocumentRelationship>> {
        let requestor = Did::new(identity).map_err(Error::InvalidIdentityDid)?;
        let policy = self
            .database
            .get_collection(collection)
            .map_err(|error| Error::CollectionLookup {
                collection: collection.to_owned(),
                source: error,
            })?
            .ok_or_else(|| Error::CollectionMissing(collection.to_owned()))?
            .schema()
            .policy
            .clone()
            .ok_or(Error::CollectionWithoutPolicy)?;
        acp::bound_to_request(&self.document_acp, context)
            .list_relationships(&requestor, &policy.id, &policy.resource_name, doc_id)
            .await
            .map_err(Error::Acp)
    }

    async fn change_nac_source_for_in_txn(
        &self,
        transaction_context: &RequestContext,
        actor_context: &RequestContext,
        edge: &query::txn::SourcePermission,
    ) -> Result<bool> {
        use query::txn::TransactionRegistry as _;
        let id = transaction_context
            .txn()
            .ok_or_else(|| query::QueryError::execution("an explicit transaction is required"))?;
        if !transaction_context.authorization_valid_now()
            || actor_context.txn().is_some_and(|actor_id| actor_id != id)
        {
            return Err(query::QueryError::permission_denied(
                "invalid transaction or actor authorization",
            )
            .into());
        }
        let job = query::txn::embedded_source_permission_job(actor_context.clone(), edge.clone())?;
        let handle = query::txn::TransactionHandle::new(id.to_string());
        Ok(self
            .registry
            .run_as(transaction_context, &handle, None, job)
            .await??)
    }
    async fn change_dac_relationship_for_in_txn(
        &self,
        transaction_context: &RequestContext,
        actor_context: &RequestContext,
        relationship: transactional::Relationship<'_>,
        delete: bool,
    ) -> Result<bool> {
        let identity = actor_context.identity().ok_or_else(|| {
            query::QueryError::permission_denied("an authenticated relationship actor is required")
        })?;
        let actor = Did::new(identity).map_err(Error::InvalidIdentityDid)?;
        if actor.is_wildcard() {
            return Err(query::QueryError::permission_denied(
                "an explicit relationship actor is required",
            )
            .into());
        }
        self.relationship_in_txn(
            transaction_context,
            actor_context,
            identity,
            relationship,
            delete,
            transactional::NodeAdmin::Admitted,
        )
        .await
    }

    #[cfg(feature = "http")]
    async fn check_doc_access(
        &self,
        context: &RequestContext,
        actor: &Did,
        permission: acp::DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool> {
        self.database
            .check_node_access_as(context, acp::nac::NodePermission::DacStatus)
            .await?;
        acp::bound_to_request(&self.document_acp, context)
            .check_doc_access(
                &acp::Identity::Authenticated(actor.clone()),
                permission,
                policy_id,
                resource_name,
                doc_id,
            )
            .await
            .map_err(Error::Acp)
    }

    async fn add_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        if context.txn().is_some() {
            return self
                .relationship_in_txn(
                    context,
                    context,
                    identity,
                    transactional::Relationship {
                        collection,
                        doc_id,
                        relation,
                        target,
                    },
                    false,
                    transactional::NodeAdmin::Refused,
                )
                .await;
        }
        let (requestor, resolved) = self
            .relationship_requestor(
                RelationshipChange::Add,
                identity,
                collection,
                [doc_id, relation, target],
            )
            .await?;
        let wrap = RelationshipChange::Add.wrap();

        if self.document_acp.supports_transactions() {
            return self
                .relationship_autocommit(
                    context,
                    identity,
                    transactional::Relationship {
                        collection,
                        doc_id,
                        relation,
                        target,
                    },
                    false,
                )
                .await
                .map_err(|error| match error {
                    Error::Acp(_) => wrap(error),
                    other => other,
                });
        }

        let added = acp::bound_to_request(&self.document_acp, context)
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
            .map_err(|error| wrap(Error::Acp(error)))?;

        // Local ACP relationships are node-local (matches Go): a grant is not
        // propagated to peers. Cross-node access control is Vera's role.
        if added {
            self.publish_document_update(&resolved.collection_id, doc_id)
                .await;
        }

        // Go returns ExistedAlready; `added` is its inverse.
        Ok(!added)
    }

    async fn delete_dac_actor_relationship(
        &self,
        context: &RequestContext,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        if context.txn().is_some() {
            return self
                .relationship_in_txn(
                    context,
                    context,
                    identity,
                    transactional::Relationship {
                        collection,
                        doc_id,
                        relation,
                        target,
                    },
                    true,
                    transactional::NodeAdmin::Refused,
                )
                .await;
        }
        let (requestor, resolved) = self
            .relationship_requestor(
                RelationshipChange::Delete,
                identity,
                collection,
                [doc_id, relation, target],
            )
            .await?;
        let wrap = RelationshipChange::Delete.wrap();

        if self.document_acp.supports_transactions() {
            return self
                .relationship_autocommit(
                    context,
                    identity,
                    transactional::Relationship {
                        collection,
                        doc_id,
                        relation,
                        target,
                    },
                    true,
                )
                .await
                .map_err(|error| match error {
                    Error::Acp(_) => wrap(error),
                    other => other,
                });
        }

        // Local ACP relationships are node-local (matches Go): a revoke is not
        // propagated to peers. Cross-node access control is Vera's role.
        acp::bound_to_request(&self.document_acp, context)
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
            .map_err(|error| wrap(Error::Acp(error)))
    }
}
