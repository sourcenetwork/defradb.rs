//! Wire types for browser-to-server document synchronization.

use serde::{Deserialize, Serialize};

pub const MAX_SYNC_BODY_BYTES: usize = 33 * 1024 * 1024;
pub const MAX_SYNC_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SYNC_BLOCK_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_SYNC_BLOCKS_PER_DOCUMENT: usize = 4096;
pub const MAX_SYNC_DOCUMENTS_PER_REQUEST: usize = 32;
pub const MAX_SYNC_ROOTS_PER_DOCUMENT: usize = 16;
pub const MAX_SYNC_RELATIONSHIPS_PER_DOCUMENT: usize = 16;
pub const MAX_SYNC_PULL_DOC_IDS: usize = 64;
pub const DEFAULT_SYNC_PAGE_SIZE: usize = 32;
pub const MAX_SYNC_PAGE_SIZE: usize = 64;
pub const MAX_SYNC_ID_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSyncBlock {
    pub cid: String,
    pub data: String,
}

/// One access grant to apply to the document it arrives with.
///
/// `target` is an actor DID or the all-actors wildcard `*`. The relationship
/// endpoint's wider subject language — cross-object edges, usersets — is
/// deliberately out of reach from a push.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSyncRelationship {
    pub relation: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSyncDocument {
    pub doc_id: String,
    pub collection_id: String,
    pub roots: Vec<String>,
    pub blocks: Vec<BrowserSyncBlock>,
    /// Grants applied while the document is registered, so a document under a
    /// policy becomes registered and readable in one step. Pushing and then
    /// granting cannot: the merge announces the document between the two, and
    /// a peer pulling on that announcement may not read it yet.
    ///
    /// Absent on the wire means empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<BrowserSyncRelationship>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSyncPull {
    #[serde(default)]
    pub doc_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSyncRequest {
    #[serde(default)]
    pub documents: Vec<BrowserSyncDocument>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull: Option<BrowserSyncPull>,
}

/// One document of a push that was not applied, and why.
///
/// A refusal is a fact about a document, not about the exchange it arrived in:
/// the rest of the push still applies and the pull still answers. Reporting it
/// rather than failing the request is what keeps a session alive through a
/// document it may not write — and naming it is what keeps that from being a
/// silent drop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSyncRefusal {
    pub doc_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSyncResponse {
    #[serde(default)]
    pub documents: Vec<BrowserSyncDocument>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Documents of the request's push that were refused. Absent on the wire
    /// means none, so a client built before this existed reads unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refused: Vec<BrowserSyncRefusal>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client built before `relationships` existed must still be read, and
    /// must still see the bytes it has always seen.
    #[test]
    fn a_document_without_relationships_round_trips_unchanged() {
        let legacy = r#"{"doc_id":"d","collection_id":"c","roots":["r"],"blocks":[]}"#;
        let document: BrowserSyncDocument = serde_json::from_str(legacy).unwrap();
        assert!(document.relationships.is_empty());
        assert_eq!(serde_json::to_string(&document).unwrap(), legacy);
    }

    /// The response gained `refused` after clients existed. An old client must
    /// still read a new server's answer, and a new client must still read an
    /// old server's.
    #[test]
    fn a_response_without_refusals_round_trips_unchanged() {
        let legacy = r#"{"documents":[]}"#;
        let response: BrowserSyncResponse = serde_json::from_str(legacy).unwrap();
        assert!(response.refused.is_empty());
        assert_eq!(serde_json::to_string(&response).unwrap(), legacy);
    }

    #[test]
    fn relationships_are_carried_when_present() {
        let document: BrowserSyncDocument = serde_json::from_str(
            r#"{"doc_id":"d","collection_id":"c","roots":["r"],"blocks":[],
                "relationships":[{"relation":"reader","target":"*"}]}"#,
        )
        .unwrap();
        assert_eq!(
            document.relationships,
            vec![BrowserSyncRelationship {
                relation: "reader".into(),
                target: "*".into(),
            }]
        );
    }
}
