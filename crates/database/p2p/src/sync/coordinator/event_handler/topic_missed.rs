//! A hub's word that messages of a topic may not have reached this node.
//!
//! The head hints lost on the topic are pulled again with the requests peers
//! already serve: a followed document's topic asks for that document's heads
//! (DocSync), a subscribed collection's topic for the collection's heads
//! (BranchableSync). The replies take the ordinary reply path, which fetches
//! whatever is missing. A plain collection has no collection heads, so its
//! peers answer with none: what a lost hint carried there arrives with the
//! document's next hint.

use blockstore::Blockstore;

use super::super::SyncCoordinator;
use crate::message::{BranchableSyncRequest, DocSyncRequest};
use crate::signing::sign_with_transport;
use crate::transport::{P2PTransport, PeerId};

impl<B, T> SyncCoordinator<B, T>
where
    B: Blockstore + 'static,
    T: P2PTransport,
{
    pub(super) async fn handle_topic_missed(&self, topic: &str, hub: &PeerId) {
        let document = self.is_followed_document(topic);
        if !document && !self.is_locally_subscribed_collection(topic) {
            return;
        }
        let peers = match self.runtime.transport.connected_peers().await {
            Ok(peers) => peers,
            Err(error) => {
                tracing::debug!(%topic, %hub, %error, "no peers to pull a missed topic from");
                return;
            }
        };
        if document {
            let mut request = DocSyncRequest::new(vec![topic.to_string()]);
            if let Err(error) = sign_with_transport(&self.runtime.transport, &mut request) {
                tracing::warn!(%topic, %error, "failed to sign the DocSync pull for a missed topic");
                return;
            }
            for peer in &peers {
                if let Err(error) = self
                    .runtime
                    .transport
                    .send_doc_sync_request(peer, request.clone())
                    .await
                {
                    tracing::debug!(%topic, peer_id = %peer, %error, "DocSync pull for a missed topic failed");
                }
            }
        } else {
            let mut request = BranchableSyncRequest::new(topic.to_string());
            if let Err(error) = sign_with_transport(&self.runtime.transport, &mut request) {
                tracing::warn!(%topic, %error, "failed to sign the BranchableSync pull for a missed topic");
                return;
            }
            for peer in &peers {
                if let Err(error) = self
                    .runtime
                    .transport
                    .send_branchable_sync_request(peer, request.clone())
                    .await
                {
                    tracing::debug!(%topic, peer_id = %peer, %error, "BranchableSync pull for a missed topic failed");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blockstore::DefraBlockstore;
    use storage::RegolithStore;

    use super::super::tests::TestTransport;
    use crate::sync::{SyncConfig, SyncCoordinator};
    use crate::transport::{PeerId, TransportEvent};

    fn pulls(transport: &TestTransport) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(pull) = transport.pulls.pop() {
            out.push(pull);
        }
        out.sort();
        out
    }

    /// A missed report pulls a followed document's heads, and a subscribed
    /// collection's, from every connected peer; a topic this node does not
    /// follow is left alone.
    #[nuclei::test(threads = 0)]
    async fn a_missed_topic_is_pulled_again_only_when_followed() {
        let peers = vec![PeerId::new("p1".to_string()), PeerId::new("p2".to_string())];
        let transport = TestTransport::new().with_connected(peers);
        let blockstore = Arc::new(DefraBlockstore::new(
            Arc::new(RegolithStore::in_memory().unwrap()),
            true,
        ));
        let (coordinator, _events) = SyncCoordinator::new(
            transport.clone(),
            blockstore,
            crate::network::test_network(),
            SyncConfig::default(),
        )
        .await
        .expect("coordinator");
        coordinator
            .followed_documents()
            .insert("bae-followed".to_string(), ());
        assert!(coordinator.subscribe_collection("c1").await.unwrap());
        let hub = PeerId::new("hub".to_string());

        for topic in ["bae-followed", "c1", "bae-foreign", "c-foreign"] {
            coordinator
                .handle_transport_event(TransportEvent::TopicMissed {
                    topic: topic.to_string(),
                    hub: hub.clone(),
                })
                .await
                .unwrap();
        }

        assert_eq!(
            pulls(&transport),
            vec![
                "collection:p1:c1",
                "collection:p2:c1",
                "doc:p1:bae-followed",
                "doc:p2:bae-followed",
            ]
        );
    }
}
