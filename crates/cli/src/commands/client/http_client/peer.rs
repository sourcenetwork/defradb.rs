//! Go's peer operations over its HTTP API.

use defra_p2p_adapter::peer::Replicator;
use reqwest::Method;
use serde_json::json;

use super::HttpClient;
use crate::error::Result;

impl HttpClient {
    /// This node's P2P addresses, each ending in `/p2p/<peer ID>`. Go's
    /// `PeerInfo`.
    pub async fn peer_info(&self) -> Result<Vec<String>> {
        let response = self.send(Method::GET, "p2p/info", &[], None, None).await?;
        Ok(response.json().await?)
    }

    /// The addresses of the peers this node is connected to. Go's
    /// `ActivePeers`.
    pub async fn active_peers(&self) -> Result<Vec<String>> {
        let response = self
            .send(Method::GET, "p2p/active-peers", &[], None, None)
            .await?;
        Ok(response.json().await?)
    }

    /// Connect to the peer at `addresses`. Go's `Connect`.
    pub async fn connect(&self, addresses: &[String]) -> Result<()> {
        let body = json!(addresses).to_string();
        self.send(Method::POST, "p2p/connect", &[], Some(body), None)
            .await
            .map(drop)
    }

    /// Close the connections to the peers at `addresses`. Go's `Disconnect`.
    pub async fn disconnect(&self, addresses: &[String]) -> Result<()> {
        let body = json!(addresses).to_string();
        self.send(Method::POST, "p2p/disconnect", &[], Some(body), None)
            .await
            .map(drop)
    }

    /// Push `collections` to the peer at `addresses`. Go's `AddReplicator`.
    pub async fn add_replicator(&self, addresses: &[String], collections: &[String]) -> Result<()> {
        let body = json!({ "Addresses": addresses, "Collections": collections }).to_string();
        self.send(Method::POST, "p2p/replicators", &[], Some(body), None)
            .await
            .map(drop)
    }

    /// Stop pushing `collections` to peer `id`, or every collection when
    /// none is named. Go's `DeleteReplicator`.
    pub async fn delete_replicator(&self, id: &str, collections: &[String]) -> Result<()> {
        let body = json!({ "ID": id, "Collections": collections }).to_string();
        self.send(Method::DELETE, "p2p/replicators", &[], Some(body), None)
            .await
            .map(drop)
    }

    /// Every replicator with its collections. Go's `ListReplicators`.
    pub async fn list_replicators(&self) -> Result<Vec<Replicator>> {
        let response = self
            .send(Method::GET, "p2p/replicators", &[], None, None)
            .await?;
        Ok(response.json().await?)
    }

    /// Subscribe to the topics of collections `names`. Go's
    /// `AddP2PCollections`.
    pub async fn add_p2p_collections(&self, names: &[String]) -> Result<()> {
        let body = json!(names).to_string();
        self.send(Method::POST, "p2p/collections", &[], Some(body), None)
            .await
            .map(drop)
    }

    /// Unsubscribe from the topics of collections `names`. Go's
    /// `DeleteP2PCollections`.
    pub async fn delete_p2p_collections(&self, names: &[String]) -> Result<()> {
        let body = json!(names).to_string();
        self.send(Method::DELETE, "p2p/collections", &[], Some(body), None)
            .await
            .map(drop)
    }

    /// The collections this node subscribes to. Go's `ListP2PCollections`.
    pub async fn list_p2p_collections(&self) -> Result<Vec<String>> {
        let response = self
            .send(Method::GET, "p2p/collections", &[], None, None)
            .await?;
        Ok(response.json().await?)
    }

    /// Subscribe to the topics of documents `doc_ids`. Go's
    /// `AddP2PDocuments`.
    pub async fn add_p2p_documents(&self, doc_ids: &[String]) -> Result<()> {
        let body = json!(doc_ids).to_string();
        self.send(Method::POST, "p2p/documents", &[], Some(body), None)
            .await
            .map(drop)
    }

    /// Unsubscribe from the topics of documents `doc_ids`. Go's
    /// `DeleteP2PDocuments`.
    pub async fn delete_p2p_documents(&self, doc_ids: &[String]) -> Result<()> {
        let body = json!(doc_ids).to_string();
        self.send(Method::DELETE, "p2p/documents", &[], Some(body), None)
            .await
            .map(drop)
    }

    /// The documents this node subscribes to. Go's `ListP2PDocuments`.
    pub async fn list_p2p_documents(&self) -> Result<Vec<String>> {
        let response = self
            .send(Method::GET, "p2p/documents", &[], None, None)
            .await?;
        Ok(response.json().await?)
    }
}
