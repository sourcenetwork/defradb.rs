use std::sync::Arc;

use identity::Did;
use zanzibar::{Relationship, Subject, ZanzibarStore};

use super::{ZanzibarDocumentACP, OWNER_RELATION};
use crate::dac::DocumentACP;
use crate::error::{Error, Result};
use crate::nac::{NacStatus, NodeACP, NodePermission};
use crate::zanzibar::store::TransactionZanzibarStore;

/// A capability is constructed only after native NAC decisions over the same
/// transaction view that will receive the relation change.
struct AdministrativeAuthorization {
    actor: Did,
    store: Arc<TransactionZanzibarStore>,
}

impl AdministrativeAuthorization {
    async fn authorize(view: datastore::NamespaceView, actor: &Did, delete: bool) -> Result<Self> {
        if actor.is_wildcard() {
            return Err(Error::PermissionDenied(
                "an actual node actor is required".into(),
            ));
        }
        let store = Arc::new(TransactionZanzibarStore::new(view));
        let nac = NodeACP::new(Arc::clone(&store));
        nac.load().await?;
        let operation = if delete {
            NodePermission::DacRelationDelete
        } else {
            NodePermission::DacRelationAdd
        };
        if nac.status().await != NacStatus::Enabled
            || !nac
                .check_permission(actor, NodePermission::DacBypass)
                .await?
            || !nac.check_permission(actor, operation).await?
        {
            return Err(Error::PermissionDenied(
                "native node-administrator relationship authority is required".into(),
            ));
        }
        Ok(Self {
            actor: actor.clone(),
            store,
        })
    }
}

impl<S> ZanzibarDocumentACP<S>
where
    S: ZanzibarStore + ?Sized + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn change_administrative_relationship(
        &self,
        view: datastore::NamespaceView,
        actor: &Did,
        target: Subject,
        policy_id: &str,
        resource: &str,
        doc_id: &str,
        relation: &str,
        delete: bool,
    ) -> Result<bool> {
        if !self.supports_transactions() {
            return Err(Error::UnsupportedAdministrativeRelationship);
        }
        let authorization = AdministrativeAuthorization::authorize(view, actor, delete).await?;
        let scoped = self.scoped(Arc::clone(&authorization.store));
        if relation == OWNER_RELATION {
            return Err(Error::InvalidRelation("owner relation is immutable".into()));
        }
        if !scoped
            .is_doc_registered(policy_id, resource, doc_id)
            .await?
        {
            return Err(Error::DocumentNotRegistered(doc_id.to_owned()));
        }
        let policy = authorization
            .store
            .get_policy(policy_id)
            .await?
            .ok_or_else(|| Error::PolicyNotFound(policy_id.to_owned()))?;
        if policy.get_relation(resource, relation).is_none() {
            return Err(Error::RelationNotFound {
                resource: resource.to_owned(),
                relation: relation.to_owned(),
            });
        }
        let relationship = Relationship::new(resource, doc_id, relation, target);
        let changed = if delete {
            authorization
                .store
                .delete_relationship(policy_id, &relationship)
                .await?
        } else {
            relationship.validate(&policy)?;
            if authorization
                .store
                .has_relationship(policy_id, resource, doc_id, relation, &relationship.subject)
                .await?
            {
                false
            } else {
                authorization
                    .store
                    .store_relationship(policy_id, &relationship)
                    .await?;
                true
            }
        };
        tracing::info!(target: "acp::audit", actor = %authorization.actor, policy = %policy_id,
            resource = %resource, document = %doc_id, relation = %relation, delete, changed,
            "native node administrator changed a document relationship");
        Ok(changed)
    }
}
