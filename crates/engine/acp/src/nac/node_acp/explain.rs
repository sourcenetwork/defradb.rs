//! Why a subject holds each node permission.

use std::collections::VecDeque;

use identity::Did;
use rapidhash::{HashSetExt, RapidHashSet};
use zanzibar::{Subject, ZanzibarStore};

use super::{NacStatus, NodeACP, NODE_OBJECT_ID};
use crate::error::{Error, Result};
use crate::nac::delegation::{PermissionProvenance, PermissionVia};
use crate::nac::permission::NodePermission;
use crate::nac::policy::{ADMIN_RELATION, NODE_POLICY_ID, NODE_RESOURCE_NAME};

const SOURCE_PREFIX: &str = "source.";

fn source_name(object: &str) -> Option<String> {
    let hex = object.strip_prefix(SOURCE_PREFIX)?;
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok()
}

impl<S: ZanzibarStore + ?Sized + 'static> NodeACP<S> {
    /// Every way `subject` holds each node permission in the stored
    /// relationships, for a requester who is `subject`, an admin, or holds
    /// `NacStatus`. The Zanzibar engine's explanation decides whether each
    /// permission is held; the stored tuples name how. A source path is the
    /// shortest chain of sources, outermost first. The requester gate reads
    /// stored relationships, so disabling NAC never discloses other subjects'
    /// grants; while NAC is not enabled every operation is allowed whatever
    /// this reports.
    pub async fn explain_subject(
        &self,
        requestor: &Did,
        subject: &Did,
    ) -> Result<Vec<PermissionProvenance>> {
        if requestor != subject
            && !self.is_admin_persisted(requestor).await?
            && !self
                .check_permission_persisted(requestor, NodePermission::NacStatus)
                .await?
        {
            return Err(Error::NotOwner {
                operation: "explain node permissions".into(),
            });
        }
        if self.status().await == NacStatus::NotConfigured {
            return Ok(Vec::new());
        }
        let owner = self.is_owner(subject).await;
        let admin = self.has_direct(ADMIN_RELATION, subject).await?;
        let engine = self.engine.snapshot();
        let mut provenance = Vec::new();
        for &permission in NodePermission::all() {
            let explanation = engine
                .explain(
                    NODE_POLICY_ID,
                    NODE_RESOURCE_NAME,
                    NODE_OBJECT_ID,
                    permission.as_str(),
                    subject,
                )
                .await?;
            if !explanation.granted {
                continue;
            }
            let before = provenance.len();
            if owner {
                provenance.push(PermissionProvenance {
                    permission,
                    via: PermissionVia::Owner,
                });
            }
            if admin {
                provenance.push(PermissionProvenance {
                    permission,
                    via: PermissionVia::Admin,
                });
            }
            if self.has_direct(permission.as_str(), subject).await? {
                provenance.push(PermissionProvenance {
                    permission,
                    via: PermissionVia::Direct,
                });
            }
            if let Some(path) = self.source_path(permission, subject).await? {
                provenance.push(PermissionProvenance {
                    permission,
                    via: PermissionVia::Source { path },
                });
            }
            if provenance.len() == before {
                return Err(Error::InvalidPolicy(format!(
                    "node permission '{permission}' is granted through no stored relationship"
                )));
            }
        }
        Ok(provenance)
    }

    async fn has_direct(&self, relation: &str, subject: &Did) -> Result<bool> {
        for candidate in [Subject::Entity(subject.clone()), Subject::Wildcard] {
            if self
                .store
                .has_relationship(
                    NODE_POLICY_ID,
                    NODE_RESOURCE_NAME,
                    NODE_OBJECT_ID,
                    relation,
                    &candidate,
                )
                .await?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The shortest chain of sources from the node to `subject` for
    /// `permission`, breadth first; each source is visited once.
    async fn source_path(
        &self,
        permission: NodePermission,
        subject: &Did,
    ) -> Result<Option<Vec<String>>> {
        let relation = permission.as_str();
        let mut visited = RapidHashSet::new();
        let mut queue = VecDeque::from([(NODE_OBJECT_ID.to_owned(), Vec::new())]);
        while let Some((object, path)) = queue.pop_front() {
            for member in self
                .store
                .get_relation_subjects(NODE_POLICY_ID, NODE_RESOURCE_NAME, &object, relation)
                .await?
            {
                match member {
                    Subject::Entity(actor) if !path.is_empty() && actor == *subject => {
                        return Ok(Some(path));
                    }
                    Subject::EntitySet {
                        resource,
                        object_id,
                        relation: member_relation,
                    } if resource == NODE_RESOURCE_NAME
                        && member_relation == relation
                        && visited.insert(object_id.clone()) =>
                    {
                        let Some(name) = source_name(&object_id) else {
                            continue;
                        };
                        let mut next = path.clone();
                        next.push(name);
                        queue.push_back((object_id, next));
                    }
                    _ => {}
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/nac_explain.rs"]
mod tests;
