//! The management-relay shapes: what `POST /p2p/manage` accepts and what
//! `POST /p2p/manage/query` answers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// [`defra_p2p_adapter::ReplicationFilter`]: one field predicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReplicationFilter {
    #[serde(rename = "Field", default, skip_serializing_if = "String::is_empty")]
    pub field: String,
    #[serde(
        rename = "Value",
        default,
        skip_serializing_if = "serde_json::Value::is_null"
    )]
    pub value: serde_json::Value,
    #[serde(
        rename = "Conditions",
        alias = "predicate",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub conditions: Option<serde_json::Map<String, serde_json::Value>>,
}

/// [`defra_p2p_adapter::ReplicationFilters`]: filters keyed by collection.
pub type ReplicationFilters = BTreeMap<String, ReplicationFilter>;

/// [`defra_p2p_adapter::ReplicatorInfo`]: one peer's replication state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ReplicatorInfo {
    pub id: Option<String>,
    pub collections: Vec<String>,
    pub address: Option<String>,
    pub status: Option<u8>,
    pub last_status_change: Option<String>,
    pub filters: ReplicationFilters,
}

/// [`defra_p2p_adapter::RemoteManageDocRef`]: a document on the peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoteManageDocRef {
    pub collection: String,
    pub doc_id: String,
}

/// [`defra_p2p_adapter::RemoteManageOp`]: a mutating request to relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "Kind")]
pub enum RemoteManageOp {
    ReplicatorAdd {
        addresses: Vec<String>,
        collection_ids: Vec<String>,
        #[serde(default)]
        filters: ReplicationFilters,
    },
    ReplicatorDelete {
        addresses: Vec<String>,
        collection_ids: Vec<String>,
    },
    CollectionAdd {
        collection_ids: Vec<String>,
    },
    CollectionRemove {
        collection_ids: Vec<String>,
    },
    DocumentAdd {
        docs: Vec<RemoteManageDocRef>,
    },
    DocumentRemove {
        docs: Vec<RemoteManageDocRef>,
    },
    PeerConnect {
        address: String,
    },
    PeerDisconnect {
        address: String,
    },
}

/// [`defra_p2p_adapter::RemoteManageQueryOp`]: a read-only request to relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "Kind")]
pub enum RemoteManageQueryOp {
    ReplicatorList,
    CollectionList,
    DocumentList,
}

/// [`defra_p2p_adapter::RemoteManageQueryResult`]: what the relayed query
/// answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(tag = "Kind")]
pub enum RemoteManageQueryResult {
    Replicators { replicators: Vec<ReplicatorInfo> },
    Strings { values: Vec<String> },
    Documents { documents: Vec<RemoteManageDocRef> },
}

fn filters_from(wire: ReplicationFilters) -> defra_p2p_adapter::ReplicationFilters {
    wire.into_iter()
        .map(|(collection, filter)| (collection, filter.into()))
        .collect()
}

impl From<ReplicationFilter> for defra_p2p_adapter::ReplicationFilter {
    fn from(filter: ReplicationFilter) -> Self {
        Self {
            field: filter.field,
            value: filter.value,
            conditions: filter.conditions,
        }
    }
}

impl From<RemoteManageDocRef> for defra_p2p_adapter::RemoteManageDocRef {
    fn from(reference: RemoteManageDocRef) -> Self {
        Self {
            collection: reference.collection,
            doc_id: reference.doc_id,
        }
    }
}

impl From<RemoteManageOp> for defra_p2p_adapter::RemoteManageOp {
    fn from(op: RemoteManageOp) -> Self {
        match op {
            RemoteManageOp::ReplicatorAdd {
                addresses,
                collection_ids,
                filters,
            } => Self::ReplicatorAdd {
                addresses,
                collection_ids,
                filters: filters_from(filters),
            },
            RemoteManageOp::ReplicatorDelete {
                addresses,
                collection_ids,
            } => Self::ReplicatorDelete {
                addresses,
                collection_ids,
            },
            RemoteManageOp::CollectionAdd { collection_ids } => {
                Self::CollectionAdd { collection_ids }
            }
            RemoteManageOp::CollectionRemove { collection_ids } => {
                Self::CollectionRemove { collection_ids }
            }
            RemoteManageOp::DocumentAdd { docs } => Self::DocumentAdd {
                docs: docs.into_iter().map(Into::into).collect(),
            },
            RemoteManageOp::DocumentRemove { docs } => Self::DocumentRemove {
                docs: docs.into_iter().map(Into::into).collect(),
            },
            RemoteManageOp::PeerConnect { address } => Self::PeerConnect { address },
            RemoteManageOp::PeerDisconnect { address } => Self::PeerDisconnect { address },
        }
    }
}

impl From<RemoteManageQueryOp> for defra_p2p_adapter::RemoteManageQueryOp {
    fn from(op: RemoteManageQueryOp) -> Self {
        match op {
            RemoteManageQueryOp::ReplicatorList => Self::ReplicatorList,
            RemoteManageQueryOp::CollectionList => Self::CollectionList,
            RemoteManageQueryOp::DocumentList => Self::DocumentList,
        }
    }
}

impl From<defra_p2p_adapter::ReplicationFilter> for ReplicationFilter {
    fn from(filter: defra_p2p_adapter::ReplicationFilter) -> Self {
        Self {
            field: filter.field,
            value: filter.value,
            conditions: filter
                .conditions
                .map(|conditions| conditions.into_iter().collect()),
        }
    }
}

impl From<defra_p2p_adapter::RemoteManageDocRef> for RemoteManageDocRef {
    fn from(reference: defra_p2p_adapter::RemoteManageDocRef) -> Self {
        Self {
            collection: reference.collection,
            doc_id: reference.doc_id,
        }
    }
}

impl From<defra_p2p_adapter::ReplicatorInfo> for ReplicatorInfo {
    fn from(info: defra_p2p_adapter::ReplicatorInfo) -> Self {
        Self {
            id: info.id,
            collections: info.collections,
            address: info.address,
            status: info.status,
            last_status_change: info.last_status_change,
            filters: info
                .filters
                .into_iter()
                .map(|(collection, filter)| (collection, filter.into()))
                .collect(),
        }
    }
}

impl From<defra_p2p_adapter::RemoteManageQueryResult> for RemoteManageQueryResult {
    fn from(result: defra_p2p_adapter::RemoteManageQueryResult) -> Self {
        match result {
            defra_p2p_adapter::RemoteManageQueryResult::Replicators { replicators } => {
                Self::Replicators {
                    replicators: replicators.into_iter().map(Into::into).collect(),
                }
            }
            defra_p2p_adapter::RemoteManageQueryResult::Strings { values } => {
                Self::Strings { values }
            }
            defra_p2p_adapter::RemoteManageQueryResult::Documents { documents } => {
                Self::Documents {
                    documents: documents.into_iter().map(Into::into).collect(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op_json() -> serde_json::Value {
        serde_json::json!({
            "Kind": "ReplicatorAdd",
            "addresses": ["/ip4/127.0.0.1/tcp/9171"],
            "collection_ids": ["bafkreia"],
            "filters": { "Users": { "Field": "name", "Value": "Ann" } },
        })
    }

    /// A request body the source accepts converts and serializes unchanged, so
    /// the mirror accepts exactly what the relay accepted.
    #[test]
    fn remote_manage_op_round_trips_through_its_source() {
        let mirror: RemoteManageOp = serde_json::from_value(op_json()).expect("mirror accepts");
        let source: defra_p2p_adapter::RemoteManageOp =
            serde_json::from_value(op_json()).expect("source accepts");
        assert_eq!(
            serde_json::to_value(defra_p2p_adapter::RemoteManageOp::from(mirror)).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }

    /// A filtered replicator's result keeps the filter shape the source has,
    /// including the omitted `Conditions`.
    #[test]
    fn query_result_matches_its_source() {
        let mut filters = defra_p2p_adapter::ReplicationFilters::new();
        filters.insert(
            "Users".to_string(),
            defra_p2p_adapter::ReplicationFilter {
                field: "name".to_string(),
                value: serde_json::json!("Ann"),
                conditions: None,
            },
        );
        let source = defra_p2p_adapter::RemoteManageQueryResult::Replicators {
            replicators: vec![defra_p2p_adapter::ReplicatorInfo {
                id: Some("12D3KooW".to_string()),
                collections: vec!["bafkreia".to_string()],
                address: None,
                status: Some(1),
                last_status_change: None,
                filters,
            }],
        };
        assert_eq!(
            serde_json::to_value(RemoteManageQueryResult::from(source.clone())).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }
}
