use std::fmt::Write as _;

use identity::Did;
use zanzibar::{Relationship, Subject, ZanzibarStore};

use super::{NacStatus, NodeACP, NODE_OBJECT_ID};
use crate::error::{Error, Result};
use crate::nac::permission::NodePermission;
use crate::nac::policy::{NODE_POLICY_ID, NODE_RESOURCE_NAME};

fn source_object(source: &str) -> Result<String> {
    if source.is_empty() || source.len() > 256 {
        return Err(Error::InvalidPolicy(
            "invalid permission grant source".into(),
        ));
    }
    let mut result = String::from("source.");
    for byte in source.as_bytes() {
        write!(&mut result, "{byte:02x}").expect("writing a string cannot fail");
    }
    Ok(result)
}

fn source_relationship(
    permission: NodePermission,
    parent_source: Option<&str>,
    subject_kind: &str,
    subject: &str,
) -> Result<Relationship> {
    let object = match parent_source {
        Some(source) => source_object(source)?,
        None => NODE_OBJECT_ID.to_owned(),
    };
    let subject = match subject_kind {
        "source" => {
            let child = source_object(subject)?;
            if child == object {
                return Err(Error::InvalidPolicy(
                    "a permission source cannot contain itself".into(),
                ));
            }
            Subject::entity_set(NODE_RESOURCE_NAME, child, permission.as_str())
        }
        "actor" if parent_source.is_some() => {
            let actor =
                Did::new(subject).map_err(|error| Error::InvalidPolicy(error.to_string()))?;
            if actor.is_wildcard() {
                return Err(Error::InvalidPolicy(
                    "source members require an explicit actor".into(),
                ));
            }
            Subject::Entity(actor)
        }
        _ => {
            return Err(Error::InvalidPolicy(
                "invalid permission source subject".into(),
            ))
        }
    };
    let relation = Relationship::new(NODE_RESOURCE_NAME, object, permission.as_str(), subject);
    Ok(relation)
}

impl NodeACP<crate::zanzibar::store::TransactionZanzibarStore> {
    /// Inspect one source edge through a validated transaction-bound range.
    pub async fn source_permission_matches(
        &self,
        permission: NodePermission,
        parent_source: Option<&str>,
        subject_kind: &str,
        subject: &str,
        delete: bool,
    ) -> Result<bool> {
        let relation = source_relationship(permission, parent_source, subject_kind, subject)?;
        let subjects = self
            .store
            .get_relation_subjects(
                NODE_POLICY_ID,
                &relation.resource,
                &relation.object_id,
                &relation.relation,
            )
            .await?;
        Ok(subjects.contains(&relation.subject) != delete)
    }

    /// Change one independent edge in the node permission's source graph.
    /// The transaction owner commits or discards the relationship.
    pub async fn change_source_permission(
        &self,
        requestor: &Did,
        permission: NodePermission,
        parent_source: Option<&str>,
        subject_kind: &str,
        subject: &str,
        delete: bool,
    ) -> Result<bool> {
        if self.status().await != NacStatus::Enabled || !self.is_admin(requestor).await? {
            return Err(Error::NotOwner {
                operation: "change source permission".into(),
            });
        }
        let relation = source_relationship(permission, parent_source, subject_kind, subject)?;
        let exists = self
            .store
            .has_relationship(
                NODE_POLICY_ID,
                &relation.resource,
                &relation.object_id,
                &relation.relation,
                &relation.subject,
            )
            .await?;
        if delete {
            if exists {
                self.store
                    .delete_relationship(NODE_POLICY_ID, &relation)
                    .await?;
            }
        } else if !exists {
            self.store
                .store_relationship(NODE_POLICY_ID, &relation)
                .await?;
        }
        Ok(if delete { exists } else { !exists })
    }
}
