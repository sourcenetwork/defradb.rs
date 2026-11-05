use identity::Did;
use zanzibar::{Relationship, Subject, ZanzibarStore};

use super::{NodeACP, NODE_OBJECT_ID};
use crate::error::{Error, Result};
use crate::nac::permission::NodePermission;
use crate::nac::policy::{NODE_POLICY_ID, NODE_RESOURCE_NAME, OWNER_RELATION};
use crate::zanzibar::store::TransactionZanzibarStore;

/// Reserved for native, transaction-bound bootstrap bindings. Document ACP
/// mutation APIs must reject this relation even when a policy declares it.
pub const BOOTSTRAP_MARKER: &str = "_nac_bootstrap_marker";
const BOOTSTRAP_RELATION: &str = "_bootstrap_owner";

const BOOTSTRAP_PERMISSIONS: &str = "_bootstrap_permissions";

fn allowed_permission(permission: NodePermission) -> bool {
    !matches!(
        permission,
        NodePermission::DacBypass
            | NodePermission::DacEnable
            | NodePermission::DacDisable
            | NodePermission::DacPurge
            | NodePermission::DacPolicyAdd
            | NodePermission::NacReEnable
            | NodePermission::NacDisable
            | NodePermission::NacPurge
            | NodePermission::NacRelationAdd
            | NodePermission::NacRelationDelete
    )
}

fn permission_subject(permission: NodePermission) -> Subject {
    Subject::entity_set(NODE_RESOURCE_NAME, NODE_OBJECT_ID, permission.as_str())
}

impl<S: ZanzibarStore + ?Sized + 'static> NodeACP<S> {
    pub(super) async fn bootstrap_permission(
        &self,
        identity: &Did,
        permission: NodePermission,
    ) -> Result<bool> {
        if !allowed_permission(permission)
            || !self
                .store
                .has_relationship(
                    NODE_POLICY_ID,
                    NODE_RESOURCE_NAME,
                    NODE_OBJECT_ID,
                    BOOTSTRAP_PERMISSIONS,
                    &permission_subject(permission),
                )
                .await?
        {
            return Ok(false);
        }
        let pointers = self
            .store
            .get_relation_subjects(
                NODE_POLICY_ID,
                NODE_RESOURCE_NAME,
                NODE_OBJECT_ID,
                BOOTSTRAP_RELATION,
            )
            .await?;
        let [Subject::EntitySet {
            resource,
            object_id,
            relation: policy_id,
        }] = pointers.as_slice()
        else {
            return Ok(false);
        };
        let Some(runtime) = self.owner.load().map(|owner| (*owner).clone()) else {
            return Ok(false);
        };
        if !self
            .store
            .has_relationship(
                policy_id,
                resource,
                object_id,
                BOOTSTRAP_MARKER,
                &Subject::Entity(runtime),
            )
            .await?
        {
            return Ok(false);
        }
        let owners = self
            .store
            .get_relation_subjects(policy_id, resource, object_id, OWNER_RELATION)
            .await?;
        Ok(matches!(owners.as_slice(), [Subject::Entity(owner)] if owner == identity))
    }
}

impl NodeACP<TransactionZanzibarStore> {
    /// Establish the first bootstrap binding in the caller's transaction. The
    /// actor is the NAC owner and the document owner is checked in this same
    /// transaction. Rebinding after deletion is deliberately unsupported: the
    /// old owner's signed authority cannot be inferred from a replacement.
    /// A direct grant predating the first binding has ambiguous provenance and
    /// must be reconciled explicitly before this operation can succeed.
    pub async fn bind_initial_bootstrap_owner(
        &self,
        actor: &Did,
        policy_id: &str,
        resource: &str,
        document: &str,
        expected_owner: &Did,
        permissions: &[NodePermission],
    ) -> Result<()> {
        if self.status().await != super::NacStatus::Enabled || !self.is_owner(actor).await {
            return Err(Error::NotOwner {
                operation: "bind bootstrap owner".into(),
            });
        }
        if [policy_id, resource, document]
            .iter()
            .any(|value| value.is_empty() || value.contains('/') || value.contains('\\'))
        {
            return Err(Error::InvalidPolicy(
                "invalid bootstrap document reference".into(),
            ));
        }
        if permissions.is_empty()
            || permissions
                .iter()
                .enumerate()
                .any(|(index, permission)| permissions[..index].contains(permission))
            || permissions
                .iter()
                .any(|permission| !allowed_permission(*permission))
        {
            return Err(Error::InvalidPolicy(
                "invalid bootstrap permission set".into(),
            ));
        }
        let target = Subject::entity_set(resource, document, policy_id);
        let pointers = self
            .store
            .get_relation_subjects(
                NODE_POLICY_ID,
                NODE_RESOURCE_NAME,
                NODE_OBJECT_ID,
                BOOTSTRAP_RELATION,
            )
            .await?;
        if !pointers.is_empty() {
            let configured = self
                .store
                .get_relation_subjects(
                    NODE_POLICY_ID,
                    NODE_RESOURCE_NAME,
                    NODE_OBJECT_ID,
                    BOOTSTRAP_PERMISSIONS,
                )
                .await?;
            if pointers.as_slice() == [target.clone()]
                && configured.len() == permissions.len()
                && permissions
                    .iter()
                    .all(|permission| configured.contains(&permission_subject(*permission)))
                && self
                    .bootstrap_permission(expected_owner, permissions[0])
                    .await?
            {
                return Ok(());
            }
            return Err(Error::InvalidPolicy(
                "bootstrap rebind requires a transaction-owned replacement authorization".into(),
            ));
        }
        if !self
            .store
            .get_relation_subjects(
                NODE_POLICY_ID,
                NODE_RESOURCE_NAME,
                NODE_OBJECT_ID,
                BOOTSTRAP_PERMISSIONS,
            )
            .await?
            .is_empty()
        {
            return Err(Error::InvalidPolicy(
                "orphaned bootstrap permission set".into(),
            ));
        }
        for permission in permissions {
            if !self
                .store
                .get_relation_subjects(
                    NODE_POLICY_ID,
                    NODE_RESOURCE_NAME,
                    NODE_OBJECT_ID,
                    permission.as_str(),
                )
                .await?
                .is_empty()
            {
                return Err(Error::InvalidPolicy(
                    "ambiguous legacy direct node grants require explicit reconciliation".into(),
                ));
            }
        }
        let owners = self
            .store
            .get_relation_subjects(policy_id, resource, document, OWNER_RELATION)
            .await?;
        if !matches!(owners.as_slice(), [Subject::Entity(owner)] if owner == expected_owner) {
            return Err(Error::InvalidPolicy(
                "bootstrap document owner does not match".into(),
            ));
        }
        self.store
            .store_relationship(
                policy_id,
                &Relationship::with_entity(resource, document, BOOTSTRAP_MARKER, actor.clone()),
            )
            .await?;
        self.store
            .store_relationship(
                NODE_POLICY_ID,
                &Relationship::new(
                    NODE_RESOURCE_NAME,
                    NODE_OBJECT_ID,
                    BOOTSTRAP_RELATION,
                    target,
                ),
            )
            .await?;
        for permission in permissions {
            self.store
                .store_relationship(
                    NODE_POLICY_ID,
                    &Relationship::new(
                        NODE_RESOURCE_NAME,
                        NODE_OBJECT_ID,
                        BOOTSTRAP_PERMISSIONS,
                        permission_subject(*permission),
                    ),
                )
                .await?;
        }
        Ok(())
    }
}
