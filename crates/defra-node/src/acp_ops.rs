//! Type-erased document-ACP relationship operations bridging DB to Node's
//! public DAC API.

use crate::{Error, Result};
use std::sync::Arc;

use identity::Did;

#[async_trait::async_trait]
pub(crate) trait AcpOps: Send + Sync {
    async fn add_dac_actor_relationship(
        &self,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool>;
    async fn delete_dac_actor_relationship(
        &self,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool>;
}

pub(crate) struct DbAcpOps<S: storage::corekv::Store + 'static> {
    database: Arc<db::DB<S>>,
    document_acp: Arc<dyn acp::DocumentACP>,
    event_bus: Arc<dyn events::Bus>,
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
        document_acp: Arc<dyn acp::DocumentACP>,
        event_bus: Arc<dyn events::Bus>,
    ) -> Self {
        Self {
            database,
            document_acp,
            event_bus,
        }
    }

    /// The requestor of a relationship change, checked the way Go's
    /// `AddDACActorRelationship` and `DeleteDACActorRelationship` check it:
    /// node access first, with no identity when the caller has none; then the
    /// collection name; then that the identity and every one of `required`
    /// (document, relation, target) are given.
    async fn relationship_requestor(
        &self,
        change: RelationshipChange,
        identity: &str,
        collection: &str,
        required: [&str; 3],
    ) -> Result<Did> {
        let did = match identity.is_empty() {
            true => None,
            false => Some(Did::new(identity).map_err(Error::InvalidIdentityDid)?),
        };
        self.database
            .check_node_access(did.as_ref(), change.permission())
            .await?;
        if collection.is_empty() {
            return Err(Error::CollectionNameEmpty);
        }
        match did {
            Some(did) if required.iter().all(|value| !value.is_empty()) => Ok(did),
            _ => Err(Error::RelationshipArgumentMissing(change.verb())),
        }
    }

    async fn relationship_target(
        &self,
        collection: &str,
        relation: &str,
        target: &str,
    ) -> Result<RelationshipTarget> {
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
            collection_id: collection_version.collection_id().to_string(),
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
    async fn add_dac_actor_relationship(
        &self,
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        let requestor = self
            .relationship_requestor(
                RelationshipChange::Add,
                identity,
                collection,
                [doc_id, relation, target],
            )
            .await?;
        let wrap = RelationshipChange::Add.wrap();
        if relation == acp::OWNER_RELATION {
            return Err(wrap(Error::OwnerRelationAdd));
        }
        let resolved = self
            .relationship_target(collection, relation, target)
            .await
            .map_err(wrap)?;

        let added = self
            .document_acp
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
        identity: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
        target: &str,
    ) -> Result<bool> {
        let requestor = self
            .relationship_requestor(
                RelationshipChange::Delete,
                identity,
                collection,
                [doc_id, relation, target],
            )
            .await?;
        let wrap = RelationshipChange::Delete.wrap();
        if relation == acp::OWNER_RELATION {
            return Err(wrap(Error::OwnerRelationDelete));
        }
        let resolved = self
            .relationship_target(collection, relation, target)
            .await
            .map_err(wrap)?;

        // Local ACP relationships are node-local (matches Go): a revoke is not
        // propagated to peers. Cross-node access control is Vera's role.
        self.document_acp
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
