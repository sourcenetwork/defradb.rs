//! Mirrors of `acp`'s document-access-control shapes.

use serde::{Deserialize, Serialize};

/// [`acp::DocumentRelationship`]: one relation a document has to a subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentRelationship {
    pub relation: String,
    pub subject: String,
}

impl From<acp::DocumentRelationship> for DocumentRelationship {
    fn from(relationship: acp::DocumentRelationship) -> Self {
        Self {
            relation: relationship.relation,
            subject: relationship.subject,
        }
    }
}

/// [`acp::nac::PermissionVia`]: how a subject holds one node permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PermissionVia {
    Owner,
    Admin,
    Direct,
    Source { path: Vec<String> },
}

/// [`acp::nac::PermissionProvenance`]: one way a subject holds a permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PermissionProvenance {
    /// The permission's wire name, as `NodePermission::as_str` spells it.
    pub permission: String,
    pub via: PermissionVia,
}

impl From<acp::nac::PermissionProvenance> for PermissionProvenance {
    fn from(provenance: acp::nac::PermissionProvenance) -> Self {
        Self {
            permission: provenance.permission.as_str().to_string(),
            via: match provenance.via {
                acp::nac::PermissionVia::Owner => PermissionVia::Owner,
                acp::nac::PermissionVia::Admin => PermissionVia::Admin,
                acp::nac::PermissionVia::Direct => PermissionVia::Direct,
                acp::nac::PermissionVia::Source { path } => PermissionVia::Source { path },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_relationship_matches_its_source() {
        let source = acp::DocumentRelationship {
            relation: "reader".to_string(),
            subject: "did:key:z6Mk".to_string(),
        };
        assert_eq!(
            serde_json::to_value(DocumentRelationship::from(source.clone())).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }

    /// The provenance serializes its permission through `as_str`, so the mirror
    /// carries that name rather than the enum's shape.
    #[test]
    fn permission_provenance_matches_its_source() {
        let source = acp::nac::PermissionProvenance {
            permission: acp::nac::NodePermission::DocumentRead,
            via: acp::nac::PermissionVia::Source {
                path: vec!["did:key:z6Mk".to_string()],
            },
        };
        assert_eq!(
            serde_json::to_value(PermissionProvenance::from(source.clone())).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }
}
