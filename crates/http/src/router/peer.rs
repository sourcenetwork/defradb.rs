//! Go's P2P client API (`PeerInfo`, `Connect`, `AddReplicator`, …) over a
//! node's [`P2POperations`]: the one implementation the HTTP server, the
//! node, FFI and WASM call. Each function checks its input and
//! answers with Go's texts and shapes; the transport work is the adapter's.

use serde::{Deserialize, Serialize};

use super::{
    ExplicitReplayCapabilityInput, P2PError, P2POperations, P2pDocumentRequest, ReplicationFilters,
};

/// Why a peer operation was refused. Each variant's text is Go's, or the
/// underlying message unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The caller may not do this on the node.
    #[error("{0}")]
    NotAuthorized(String),
    /// The request was refused; its message, unchanged.
    #[error("{0}")]
    Request(String),
    /// The node was started without a network. Go's `ErrNoP2P`.
    #[error("no p2p system configured")]
    NoP2P,
}

pub type Result<T> = std::result::Result<T, Error>;

/// A peer this node pushes collections to: Go's `client.Replicator`, whose
/// fields marshal capitalised.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replicator {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Addresses", default)]
    pub addresses: Vec<String>,
    #[serde(rename = "CollectionIDs", default)]
    pub collection_ids: Vec<String>,
    /// Active is 0, inactive is 1, as Go's `client.ReplicatorStatus`.
    #[serde(rename = "Status", default)]
    pub status: u8,
    #[serde(rename = "LastStatusChange", default)]
    pub last_status_change: String,
    /// Rust-only: the filter each filtered collection replicates through.
    #[serde(
        rename = "Filters",
        default,
        skip_serializing_if = "serde_json::Map::is_empty"
    )]
    pub filters: serde_json::Map<String, serde_json::Value>,
}

/// Go's `ErrAddressesEmpty`.
pub const ADDRESSES_EMPTY: &str = "addresses cannot be empty";

/// A transport refusal as the client sees it: its message, unchanged.
pub fn refused(error: P2PError) -> Error {
    match error {
        P2PError::Unauthorized(message) => Error::NotAuthorized(message),
        P2PError::NotFound(message)
        | P2PError::InvalidInput(message)
        | P2PError::Unsupported(message)
        | P2PError::Transport(message)
        | P2PError::Internal(message) => Error::Request(message),
    }
}

/// The Rust-only parts of an `AddReplicator`: filtered replication, explicit
/// replay capabilities, and the DID that must authorize the replicator.
#[derive(Clone, Debug, Default)]
pub struct ReplicatorExtras {
    pub filters: ReplicationFilters,
    pub explicit_replay_capabilities: Vec<ExplicitReplayCapabilityInput>,
    pub expected_authorizer_did: Option<String>,
}

/// Go's `PeerInfo`: every listen address with `/p2p/<peer ID>` appended.
pub async fn peer_info<P>(p2p: &P) -> Result<Vec<String>>
where
    P: P2POperations + ?Sized,
{
    let peer_id = p2p.local_peer_id().await.map_err(refused)?;
    let addresses = p2p.listen_addresses().await.map_err(refused)?;
    Ok(addresses
        .into_iter()
        .map(|address| match address.starts_with('/') {
            true => format!("{address}/p2p/{peer_id}"),
            false => address,
        })
        .collect())
}

/// Go's `ActivePeers`.
pub async fn active_peers<P>(p2p: &P) -> Result<Vec<String>>
where
    P: P2POperations + ?Sized,
{
    p2p.connected_peers().await.map_err(refused)
}

fn addresses_given(addresses: &[String]) -> Result<()> {
    match addresses.iter().any(|address| address.trim().is_empty()) || addresses.is_empty() {
        true => Err(Error::Request(ADDRESSES_EMPTY.to_owned())),
        false => Ok(()),
    }
}

/// Go's `Connect`.
pub async fn connect<P>(p2p: &P, addresses: &[String]) -> Result<()>
where
    P: P2POperations + ?Sized,
{
    addresses_given(addresses)?;
    for address in addresses {
        p2p.connect_peer(address).await.map_err(refused)?;
    }
    Ok(())
}

/// Go's `Disconnect`.
pub async fn disconnect<P>(p2p: &P, addresses: &[String]) -> Result<()>
where
    P: P2POperations + ?Sized,
{
    addresses_given(addresses)?;
    for address in addresses {
        p2p.disconnect_peer(address).await.map_err(refused)?;
    }
    Ok(())
}

/// Go's `AddReplicator`: the peer is the one `addresses` name.
pub async fn add_replicator<P>(
    p2p: &P,
    addresses: &[String],
    collections: &[String],
    extras: ReplicatorExtras,
) -> Result<()>
where
    P: P2POperations + ?Sized,
{
    p2p.add_replicator(
        collections.to_vec(),
        addresses.first().map(String::as_str),
        extras.filters,
        extras.explicit_replay_capabilities,
        extras.expected_authorizer_did.as_deref(),
    )
    .await
    .map_err(refused)
}

/// Go's `DeleteReplicator`.
pub async fn delete_replicator<P>(p2p: &P, id: &str, collections: &[String]) -> Result<()>
where
    P: P2POperations + ?Sized,
{
    p2p.remove_replicator(collections.to_vec(), Some(id))
        .await
        .map_err(refused)
}

/// Go's `ListReplicators`.
pub async fn list_replicators<P>(p2p: &P) -> Result<Vec<Replicator>>
where
    P: P2POperations + ?Sized,
{
    let replicators = p2p.get_replicators().await.map_err(refused)?;
    Ok(replicators
        .into_iter()
        .map(|replicator| Replicator {
            id: replicator.id.unwrap_or_default(),
            addresses: replicator.address.into_iter().collect(),
            collection_ids: replicator.collections,
            status: replicator.status.unwrap_or(0),
            last_status_change: replicator
                .last_status_change
                .unwrap_or_else(|| "0001-01-01T00:00:00Z".to_owned()),
            filters: match serde_json::to_value(replicator.filters) {
                Ok(serde_json::Value::Object(filters)) => filters,
                _ => serde_json::Map::new(),
            },
        })
        .collect())
}

/// Go's `AddP2PCollections`.
pub async fn add_p2p_collections<P>(p2p: &P, names: &[String]) -> Result<()>
where
    P: P2POperations + ?Sized,
{
    p2p.add_collections(names.to_vec()).await.map_err(refused)
}

/// Go's `DeleteP2PCollections`.
pub async fn delete_p2p_collections<P>(p2p: &P, names: &[String]) -> Result<()>
where
    P: P2POperations + ?Sized,
{
    p2p.remove_collections(names.to_vec())
        .await
        .map_err(refused)
}

/// Go's `ListP2PCollections`.
pub async fn list_p2p_collections<P>(p2p: &P) -> Result<Vec<String>>
where
    P: P2POperations + ?Sized,
{
    p2p.get_collections().await.map_err(refused)
}

fn documents(doc_ids: &[String]) -> Vec<P2pDocumentRequest> {
    doc_ids
        .iter()
        .map(|doc_id| P2pDocumentRequest {
            collection: String::new(),
            doc_id: doc_id.clone(),
        })
        .collect()
}

/// Go's `AddP2PDocuments`.
pub async fn add_p2p_documents<P>(p2p: &P, doc_ids: &[String]) -> Result<()>
where
    P: P2POperations + ?Sized,
{
    p2p.add_documents(documents(doc_ids)).await.map_err(refused)
}

/// Go's `DeleteP2PDocuments`.
pub async fn delete_p2p_documents<P>(p2p: &P, doc_ids: &[String]) -> Result<()>
where
    P: P2POperations + ?Sized,
{
    p2p.remove_documents(documents(doc_ids))
        .await
        .map_err(refused)
}

/// Go's `ListP2PDocuments`.
pub async fn list_p2p_documents<P>(p2p: &P) -> Result<Vec<String>>
where
    P: P2POperations + ?Sized,
{
    let documents = p2p.get_documents().await.map_err(refused)?;
    Ok(documents
        .into_iter()
        .map(|document| document.doc_id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_addresses_are_refused_with_go_text() {
        assert_eq!(
            addresses_given(&[]).unwrap_err().to_string(),
            ADDRESSES_EMPTY
        );
        assert_eq!(
            addresses_given(&[" ".to_owned()]).unwrap_err().to_string(),
            ADDRESSES_EMPTY
        );
        assert!(addresses_given(&["/ip4/127.0.0.1/tcp/1".to_owned()]).is_ok());
    }
}
