//! The checks a relationship change runs before it reaches document ACP, in
//! the order Go's `AddDACActorRelationship` and `DeleteDACActorRelationship`
//! run them.

use acp::nac::NodePermission;
use identity::Did;
use query::{CollectionProvider, DocFetcher};
use storage::corekv::Store;

use super::{DocAcpError, DocumentRelationships, Result};

/// Whether a relationship change adds or deletes.
#[derive(Clone, Copy)]
pub(super) enum ChangeKind {
    Add,
    Delete,
}

impl ChangeKind {
    pub(super) fn of(delete: bool) -> Self {
        if delete {
            Self::Delete
        } else {
            Self::Add
        }
    }

    pub(super) fn verb(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Delete => "delete",
        }
    }

    pub(super) fn permission(self) -> NodePermission {
        match self {
            Self::Add => NodePermission::DacRelationAdd,
            Self::Delete => NodePermission::DacRelationDelete,
        }
    }

    /// Go's error for changing the owner relation.
    pub(super) fn owner_error(self) -> DocAcpError {
        match self {
            Self::Add => DocAcpError::OwnerRelationAdd,
            Self::Delete => DocAcpError::OwnerRelationDelete,
        }
    }

    /// Wraps an error from the policy side of the change in Go's
    /// `failed to <verb> document actor relationship with acp`.
    pub(super) fn wrap(self) -> impl Fn(DocAcpError) -> DocAcpError + Copy {
        // What the collection lookup finds before ACP is asked is Go's own
        // error, unwrapped.
        move |source| match source {
            DocAcpError::CollectionWithoutPolicy
            | DocAcpError::CollectionMissing(_)
            | DocAcpError::CollectionLookup { .. } => source,
            source => DocAcpError::RelationshipAcp {
                verb: self.verb(),
                source: Box::new(source),
            },
        }
    }
}

/// Everything a relationship call needs from the collection and its policy.
pub(super) struct RelationshipTarget {
    pub(super) target: acp::Subject,
    pub(super) policy_id: String,
    pub(super) resource_name: String,
    pub(super) collection_id: String,
    pub(super) managing_relations: Vec<String>,
}

impl<S> DocumentRelationships<S>
where
    S: Store + 'static,
{
    /// The requestor of a relationship change and what it targets, checked in
    /// Go's order: the change's node access, that document ACP is available,
    /// then `get-collection` (Go reads the collection as the caller), the
    /// collection name, the collection and its policy; only then that the
    /// identity and every one of `required` (document, relation, target) are
    /// given, and that the relation is not the owner.
    pub(super) async fn requestor(
        &self,
        change: ChangeKind,
        identity: &str,
        collection: &str,
        required: [&str; 3],
    ) -> Result<(Did, RelationshipTarget)> {
        let database = self.registry.db();
        let did = match identity.is_empty() {
            true => None,
            false => Some(Did::new(identity).map_err(DocAcpError::InvalidIdentityDid)?),
        };
        database
            .check_node_access(did.as_ref(), change.permission())
            .await?;
        if database.dac_policies().is_none() {
            return Err(crate::DacPolicyError::AcpNotAvailable.into());
        }
        database
            .check_node_access(did.as_ref(), NodePermission::CollectionGet)
            .await?;
        if collection.is_empty() {
            return Err(DocAcpError::CollectionNameEmpty);
        }
        let [_, relation, target] = required;
        let wrap = change.wrap();
        let policy = self.collection_policy(collection).map_err(wrap)?;
        let did = match did {
            Some(did) if required.iter().all(|value| !value.is_empty()) => did,
            _ => return Err(DocAcpError::RelationshipArgumentMissing(change.verb())),
        };
        if relation == acp::OWNER_RELATION {
            return Err(wrap(change.owner_error()));
        }
        let target = acp::parse_target_subject(target)
            .map_err(DocAcpError::InvalidTarget)
            .map_err(wrap)?;
        let loaded = self
            .registry
            .db()
            .dac_policies()
            .ok_or(crate::DacPolicyError::AcpNotAvailable)?
            .get(&policy.1.id)
            .await
            .map_err(|error| wrap(error.into()))?
            .ok_or_else(|| DocAcpError::PolicyNotFound(policy.1.id.clone()))
            .map_err(wrap)?;
        let resolved = self
            .relationship_target(policy, relation, target, &loaded)
            .map_err(wrap)?;
        Ok((did, resolved))
    }

    /// Go's relationship checks, decided in a transaction's turn.
    /// `change` is the node permission the relationship operation needs.
    pub(super) async fn requestor_in_txn(
        &self,
        fetcher: &dyn DocFetcher,
        collections: &dyn CollectionProvider,
        change: ChangeKind,
        identity: &str,
        collection: &str,
        required: [&str; 3],
    ) -> Result<(Did, RelationshipTarget)> {
        let database = self.registry.db();
        let did = match identity.is_empty() {
            true => None,
            false => Some(Did::new(identity).map_err(DocAcpError::InvalidIdentityDid)?),
        };
        check_txn_node_permission(fetcher, database, did.as_ref(), change.permission()).await?;
        if database.dac_policies().is_none() {
            return Err(crate::DacPolicyError::AcpNotAvailable.into());
        }
        check_txn_node_permission(
            fetcher,
            database,
            did.as_ref(),
            NodePermission::CollectionGet,
        )
        .await?;
        if collection.is_empty() {
            return Err(DocAcpError::CollectionNameEmpty);
        }
        let definition = collections
            .get_collection(collection)
            .await
            .map_err(|error| DocAcpError::CollectionLookup {
                collection: collection.to_owned(),
                source: crate::Error::Query(error),
            })?
            .ok_or_else(|| DocAcpError::CollectionMissing(collection.to_owned()))?;
        let policy = definition
            .policy
            .as_ref()
            .ok_or(DocAcpError::CollectionWithoutPolicy)?;
        let [_, relation, target] = required;
        let wrap = change.wrap();
        let did = match did {
            Some(did) if required.iter().all(|value| !value.is_empty()) => did,
            _ => return Err(DocAcpError::RelationshipArgumentMissing(change.verb())),
        };
        if relation == acp::OWNER_RELATION {
            return Err(wrap(change.owner_error()));
        }
        let target = acp::parse_target_subject(target)
            .map_err(DocAcpError::InvalidTarget)
            .map_err(wrap)?;
        let loaded = fetcher
            .transaction_dac_policy(&policy.id)
            .await
            .map_err(|error| wrap(DocAcpError::Query(error)))?
            .ok_or_else(|| DocAcpError::PolicyNotFound(policy.id.clone()))
            .map_err(wrap)?;
        let resolved = self
            .relationship_target(
                (definition.collection_id.clone(), policy.clone()),
                relation,
                target,
                &loaded,
            )
            .map_err(wrap)?;
        Ok((did, resolved))
    }

    /// The collection ID and policy of collection `collection`, which a
    /// relationship needs.
    pub(super) fn collection_policy(
        &self,
        collection: &str,
    ) -> Result<(String, schema::PolicyDescription)> {
        let collection_version = self
            .registry
            .db()
            .get_collection(collection)
            .map_err(|error| DocAcpError::CollectionLookup {
                collection: collection.to_owned(),
                source: error,
            })?
            .ok_or_else(|| DocAcpError::CollectionMissing(collection.to_owned()))?;
        let Some(policy) = collection_version.schema().policy.clone() else {
            return Err(DocAcpError::CollectionWithoutPolicy);
        };
        Ok((collection_version.collection_id().to_string(), policy))
    }

    /// The parsed target and policy facts a relationship change on `relation`
    /// needs, under collection policy `policy`.
    fn relationship_target(
        &self,
        (collection_id, policy): (String, schema::PolicyDescription),
        relation: &str,
        target: acp::Subject,
        loaded: &acp::Policy,
    ) -> Result<RelationshipTarget> {
        let resource = loaded
            .get_resource(&policy.resource_name)
            .ok_or_else(|| DocAcpError::ResourceNotFound(policy.resource_name.clone()))?;
        if resource.get_relation(relation).is_none() {
            return Err(DocAcpError::RelationNotFound {
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
}

/// One of Go's first three relationship checks, decided from the NAC state the
/// transaction reads. The node's own identity is admitted, as outside one.
async fn check_txn_node_permission<S>(
    fetcher: &dyn DocFetcher,
    database: &crate::DB<S>,
    actor: Option<&Did>,
    permission: NodePermission,
) -> Result<()>
where
    S: Store + 'static,
{
    if database.nac_manager().is_none() {
        return Ok(());
    }
    if database
        .node_did()
        .is_some_and(|owner| actor == Some(&owner))
    {
        return Ok(());
    }
    let actor = actor.cloned().unwrap_or_else(Did::wildcard);
    match fetcher
        .check_bound_node_permission(&actor, permission, false)
        .await?
    {
        Some(true) => Ok(()),
        Some(false) => Err(crate::Error::NotAuthorized {
            permission: permission.as_str().to_owned(),
        }
        .into()),
        None => Err(query::QueryError::execution(
            "transaction-bound node authorization is unavailable",
        )
        .into()),
    }
}
