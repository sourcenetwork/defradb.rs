//! Dispatcher for [`TransportEvent::GossipRawMessage`] that routes the KMS
//! encryption topic to the KMS transport and pubsub_rpc gossipsub payloads to
//! the appropriate `TopicHandler`.

use blockstore::Blockstore;
use tracing::debug;

use super::super::SyncCoordinator;
use crate::error::Result;
use crate::transport::{P2PTransport, PeerId};

impl<B: Blockstore + 'static, T: P2PTransport> SyncCoordinator<B, T> {
    /// `publisher` is the peer that published the message, not the neighbour
    /// that relayed it: a request is answered on its publisher's `_response`
    /// topic, and a relay's topic reaches no one waiting for the reply (#2773).
    pub(super) async fn handle_gossip_raw_message(
        &self,
        publisher: PeerId,
        topic: String,
        data: Vec<u8>,
    ) -> Result<()> {
        // KMS short-circuit, on any transport that emits raw gossip. Go layers
        // the go-libp2p-pubsub-rpc protocol on the `encryption` topic: bare-CBOR
        // requests on `encryption`, and reply envelopes on the per-peer
        // `encryption/<caller>/_response` sub-topic. Both must reach the KMS
        // transport's dispatcher (not the doc-sync/branchable pubsub_rpc path),
        // so match the base topic and its `_response` sub-topics here.
        #[cfg(feature = "kms")]
        {
            let is_kms_base = topic == crate::topics::ENCRYPTION_TOPIC;
            let is_kms_response = topic
                .starts_with(&format!("{}/", crate::topics::ENCRYPTION_TOPIC))
                && topic.ends_with("/_response");
            if is_kms_base || is_kms_response {
                if let Some(transport) = self.kms_transport.get() {
                    transport
                        .dispatch_incoming(publisher.to_string(), topic, data)
                        .await;
                } else {
                    debug!(
                        topic = %topic,
                        "KMS message arrived but no KMS transport installed; dropping"
                    );
                }
                return Ok(());
            }
        }

        let _ = (publisher, data);
        debug!(topic = %topic, "raw topic message no service reads; dropping");
        Ok(())
    }
}
