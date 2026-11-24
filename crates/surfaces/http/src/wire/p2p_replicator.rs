//! The replicator listing, as `GET /p2p/replicators` answers it.

use serde::Serialize;

/// [`defra_p2p_adapter::peer::Replicator`]: one configured replicator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Replicator {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Addresses")]
    pub addresses: Vec<String>,
    #[serde(rename = "CollectionIDs")]
    pub collection_ids: Vec<String>,
    /// Active is 0, inactive is 1, as Go's `client.ReplicatorStatus`.
    #[serde(rename = "Status")]
    pub status: u8,
    #[serde(rename = "LastStatusChange")]
    pub last_status_change: String,
    /// The filter each filtered collection replicates through.
    #[serde(rename = "Filters", skip_serializing_if = "serde_json::Map::is_empty")]
    pub filters: serde_json::Map<String, serde_json::Value>,
}

impl From<defra_p2p_adapter::peer::Replicator> for Replicator {
    fn from(replicator: defra_p2p_adapter::peer::Replicator) -> Self {
        Self {
            id: replicator.id,
            addresses: replicator.addresses,
            collection_ids: replicator.collection_ids,
            status: replicator.status,
            last_status_change: replicator.last_status_change,
            filters: replicator.filters,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mirror writes the capitalised names and omits the empty filter map,
    /// as the source does.
    #[test]
    fn replicator_matches_its_source() {
        let source = defra_p2p_adapter::peer::Replicator {
            id: "12D3KooW".to_string(),
            addresses: vec!["/ip4/127.0.0.1/tcp/9171".to_string()],
            collection_ids: vec!["bafkreia".to_string()],
            status: 0,
            last_status_change: "2026-01-01T00:00:00Z".to_string(),
            filters: serde_json::Map::new(),
        };
        assert_eq!(
            serde_json::to_value(Replicator::from(source.clone())).unwrap(),
            serde_json::to_value(&source).unwrap()
        );
    }
}
