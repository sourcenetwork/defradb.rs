//! Mirrors of `query`'s response and request shapes.

use serde::{Deserialize, Serialize};

/// [`query::documents::DocumentsResult`]: what a filtered write reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct DocumentsResult {
    #[serde(rename = "Count")]
    pub count: usize,
    #[serde(rename = "DocIDs")]
    pub doc_ids: Vec<String>,
}

impl From<query::documents::DocumentsResult> for DocumentsResult {
    fn from(result: query::documents::DocumentsResult) -> Self {
        Self {
            count: result.count,
            doc_ids: result.doc_ids,
        }
    }
}

/// [`query::txn::SourcePermission`]: one source-grant turn's input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SourcePermission {
    pub permission: String,
    pub parent_source: Option<String>,
    pub subject_kind: String,
    pub subject: String,
    pub delete: bool,
}

impl From<SourcePermission> for query::txn::SourcePermission {
    fn from(permission: SourcePermission) -> Self {
        Self {
            permission: permission.permission,
            parent_source: permission.parent_source,
            subject_kind: permission.subject_kind,
            subject: permission.subject,
            delete: permission.delete,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mirror and the domain type serialize identically, so the document
    /// describes what the endpoint sends.
    #[test]
    fn documents_result_matches_its_source() {
        let source = query::documents::DocumentsResult::from(vec![
            "bae-c0ffee".to_string(),
            "bae-deadbeef".to_string(),
        ]);
        assert_eq!(
            serde_json::to_value(DocumentsResult::from(source.clone())).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }

    #[test]
    fn source_permission_matches_its_source() {
        let json = serde_json::json!({
            "permission": "read",
            "parent_source": null,
            "subject_kind": "user",
            "subject": "did:key:z6Mk",
            "delete": false,
        });
        let mirror: SourcePermission = serde_json::from_value(json.clone()).unwrap();
        let domain: query::txn::SourcePermission = serde_json::from_value(json).unwrap();
        assert_eq!(
            serde_json::to_value(query::txn::SourcePermission::from(mirror)).unwrap(),
            serde_json::to_value(&domain).unwrap()
        );
    }
}
