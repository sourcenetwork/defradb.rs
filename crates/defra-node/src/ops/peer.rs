//! Go's peer operations (`PeerInfo`, `Connect`, `AddReplicator`, …) as the
//! caller `context` names, on the node's network edge: P2P work is never a
//! database worker's.

use std::sync::Arc;

use defra_core::RequestContext;
use defra_http::router::peer::{self, Replicator};

use crate::{Error, Node, Result};

impl Node {
    /// This node's P2P addresses, each ending in `/p2p/<peer ID>`; none
    /// without a network. Go's `PeerInfo`.
    pub async fn peer_info_as(&self, context: &RequestContext) -> Result<Vec<String>> {
        match self.peer(context) {
            Ok(p2p) => {
                self.at_edge(async move { peer::peer_info(&*p2p).await })
                    .await
            }
            Err(_) => Ok(Vec::new()),
        }
    }

    /// The addresses of the peers this node is connected to. Go's
    /// `ActivePeers`.
    pub async fn active_peers_as(&self, context: &RequestContext) -> Result<Vec<String>> {
        let p2p = self.peer(context)?;
        self.at_edge(async move { peer::active_peers(&*p2p).await })
            .await
    }

    /// Connect to the peer at `addresses`. Go's `Connect`.
    pub async fn connect_as(&self, context: &RequestContext, addresses: &[String]) -> Result<()> {
        let (p2p, addresses) = (self.peer(context)?, addresses.to_vec());
        self.at_edge(async move { peer::connect(&*p2p, &addresses).await })
            .await
    }

    /// Close the connections to the peers at `addresses`. Go's `Disconnect`.
    pub async fn disconnect_as(
        &self,
        context: &RequestContext,
        addresses: &[String],
    ) -> Result<()> {
        let (p2p, addresses) = (self.peer(context)?, addresses.to_vec());
        self.at_edge(async move { peer::disconnect(&*p2p, &addresses).await })
            .await
    }

    /// Push `collections` to the peer at `addresses`, now and on every
    /// write, adding them to its replicator if it has one. Go's
    /// `AddReplicator`.
    pub async fn add_replicator_as(
        &self,
        context: &RequestContext,
        addresses: &[String],
        collections: &[String],
    ) -> Result<()> {
        let p2p = self.peer(context)?;
        let (addresses, collections) = (addresses.to_vec(), collections.to_vec());
        let extras = peer::ReplicatorExtras {
            expected_authorizer_did: context.identity().map(str::to_owned),
            ..Default::default()
        };
        self.at_edge(
            async move { peer::add_replicator(&*p2p, &addresses, &collections, extras).await },
        )
        .await
    }

    /// Stop pushing `collections` to peer `id`, or every collection when
    /// none is named. Go's `DeleteReplicator`.
    pub async fn delete_replicator_as(
        &self,
        context: &RequestContext,
        id: &str,
        collections: &[String],
    ) -> Result<()> {
        let (p2p, id, collections) = (self.peer(context)?, id.to_owned(), collections.to_vec());
        self.at_edge(async move { peer::delete_replicator(&*p2p, &id, &collections).await })
            .await
    }

    /// Every replicator with its collections. Go's `ListReplicators`.
    pub async fn list_replicators_as(&self, context: &RequestContext) -> Result<Vec<Replicator>> {
        let p2p = self.peer(context)?;
        self.at_edge(async move { peer::list_replicators(&*p2p).await })
            .await
    }

    /// Subscribe to the topics of collections `names`. Go's
    /// `AddP2PCollections`.
    pub async fn add_p2p_collections_as(
        &self,
        context: &RequestContext,
        names: &[String],
    ) -> Result<()> {
        let (p2p, names) = (self.peer(context)?, names.to_vec());
        self.at_edge(async move { peer::add_p2p_collections(&*p2p, &names).await })
            .await
    }

    /// Unsubscribe from the topics of collections `names`. Go's
    /// `DeleteP2PCollections`.
    pub async fn delete_p2p_collections_as(
        &self,
        context: &RequestContext,
        names: &[String],
    ) -> Result<()> {
        let (p2p, names) = (self.peer(context)?, names.to_vec());
        self.at_edge(async move { peer::delete_p2p_collections(&*p2p, &names).await })
            .await
    }

    /// The collections this node subscribes to. Go's `ListP2PCollections`.
    pub async fn list_p2p_collections_as(&self, context: &RequestContext) -> Result<Vec<String>> {
        let p2p = self.peer(context)?;
        self.at_edge(async move { peer::list_p2p_collections(&*p2p).await })
            .await
    }

    /// Subscribe to the topics of documents `doc_ids`. Go's
    /// `AddP2PDocuments`.
    pub async fn add_p2p_documents_as(
        &self,
        context: &RequestContext,
        doc_ids: &[String],
    ) -> Result<()> {
        let (p2p, doc_ids) = (self.peer(context)?, doc_ids.to_vec());
        self.at_edge(async move { peer::add_p2p_documents(&*p2p, &doc_ids).await })
            .await
    }

    /// Unsubscribe from the topics of documents `doc_ids`. Go's
    /// `DeleteP2PDocuments`.
    pub async fn delete_p2p_documents_as(
        &self,
        context: &RequestContext,
        doc_ids: &[String],
    ) -> Result<()> {
        let (p2p, doc_ids) = (self.peer(context)?, doc_ids.to_vec());
        self.at_edge(async move { peer::delete_p2p_documents(&*p2p, &doc_ids).await })
            .await
    }

    /// The documents this node subscribes to. Go's `ListP2PDocuments`.
    pub async fn list_p2p_documents_as(&self, context: &RequestContext) -> Result<Vec<String>> {
        let p2p = self.peer(context)?;
        self.at_edge(async move { peer::list_p2p_documents(&*p2p).await })
            .await
    }

    /// The P2P operations serving `context`, or Go's `ErrNoP2P`.
    fn peer(&self, context: &RequestContext) -> Result<Arc<dyn defra_http::P2POperations>> {
        self.p2p_system()
            .map(|system| system.ops_for(context))
            .ok_or(Error::Peer(peer::Error::NoP2P))
    }

    /// Run `work` on the node's network edge.
    async fn at_edge<F, T>(&self, work: F) -> Result<T>
    where
        F: std::future::Future<Output = peer::Result<T>> + Send + 'static,
        T: Send + 'static,
    {
        let Some(network) = &self.network else {
            return Ok(work.await?);
        };
        Ok(network.edge().spawn(work).await??)
    }
}
