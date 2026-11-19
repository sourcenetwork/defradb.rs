//! Held-bounded delegation of node relationship changes, and the explained
//! provenance of a subject's node permissions.
//!
//! Model: `proofs/lean/NodeAcpDelegation.lean` (`admits`, `authorized`,
//! `delegated_no_escalation`).

use serde::{Deserialize, Serialize};

use super::permission::NodePermission;

/// The permissions that widen authority or switch enforcement off. A
/// delegate never hands these out or takes them away; the Lean `adminClass`
/// docstring records why each is here and why every other permission is not.
pub const ADMIN_CLASS: &[NodePermission] = &[
    NodePermission::DacBypass,
    NodePermission::NacRelationAdd,
    NodePermission::NacRelationDelete,
    NodePermission::NacDisable,
    NodePermission::NacReEnable,
    NodePermission::NacPurge,
    NodePermission::DacEnable,
    NodePermission::DacDisable,
    NodePermission::DacPurge,
];

impl NodePermission {
    /// Whether this permission is in [`ADMIN_CLASS`].
    pub fn is_admin_class(&self) -> bool {
        ADMIN_CLASS.contains(self)
    }
}

/// The relation a node relationship change targets. `owner` is never one:
/// the owner is set once, when NAC is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NacRelation {
    Admin,
    Permission(NodePermission),
}

impl NacRelation {
    /// The relation's name in the node policy.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => super::policy::ADMIN_RELATION,
            Self::Permission(permission) => permission.as_str(),
        }
    }
}

/// What the gate observes for one requested change, as Lean's `Facts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelegationFacts {
    /// The requester is the owner or an admin.
    pub requestor_admin: bool,
    pub relation: NacRelation,
    /// The requester holds `NacRelationAdd` (add) or `NacRelationDelete` (delete).
    pub holds_operation: bool,
    /// The requester holds the permission the change carries.
    pub holds_permission: bool,
    /// The change names everyone (`*`).
    pub wildcard_subject: bool,
    /// The change names the owner or an admin.
    pub privileged_subject: bool,
    pub delete: bool,
}

impl DelegationFacts {
    /// An admin may change anything. A delegate may change one permission
    /// outside the admin class that it holds, with the operation held too,
    /// for a concrete subject, and never delete an owner's or admin's tuple.
    pub fn admits(&self) -> bool {
        if self.requestor_admin {
            return true;
        }
        let NacRelation::Permission(permission) = self.relation else {
            return false;
        };
        !permission.is_admin_class()
            && self.holds_operation
            && self.holds_permission
            && !self.wildcard_subject
            && !(self.delete && self.privileged_subject)
    }
}

/// How a subject holds one node permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PermissionVia {
    /// The subject is the node's owner.
    Owner,
    /// The subject is an admin, directly or because `*` is.
    Admin,
    /// A direct grant names the subject, or everyone (`*`).
    Direct,
    /// A chain of named sources from the node to the subject, outermost first.
    Source { path: Vec<String> },
}

/// One way a subject holds one node permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionProvenance {
    #[serde(serialize_with = "wire_name", deserialize_with = "from_wire_name")]
    pub permission: NodePermission,
    pub via: PermissionVia,
}

fn wire_name<S>(permission: &NodePermission, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(permission.as_str())
}

fn from_wire_name<'de, D>(deserializer: D) -> Result<NodePermission, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let name = String::deserialize(deserializer)?;
    NodePermission::parse(&name)
        .ok_or_else(|| serde::de::Error::custom(format!("unknown node permission '{name}'")))
}

#[cfg(test)]
#[path = "../../tests/unit/nac_delegation.rs"]
mod tests;
