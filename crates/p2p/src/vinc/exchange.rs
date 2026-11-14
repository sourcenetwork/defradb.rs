//! Peer exchange: a node hands a new peer the records of its other peers,
//! and the peer dials them, so peers of peers connect, as libp2p's Kademlia
//! and iroh's gossip connect them.
//!
//! The records are the signed ones each peer sent on its own connection; the
//! receiver keeps them unverified and vinc verifies a record before a dial
//! uses it, so a forged one fails that dial and nothing else.

use std::rc::Rc;

use tracing::debug;
use vinc::{Connection, Hosted, NodeId};

use crate::error::Result;

use super::addr::node_to_peer_id;
use super::endpoint::Node;
use super::services::{self, MAX_SHARED_PEERS};
use super::wire;

/// The records of a peer exchange: each peer's id and its signed record.
type Shared = Vec<(String, Vec<u8>)>;

impl<H: Hosted + 'static> Node<H> {
    /// Give the peer of `connection` the records of this node's other peers.
    pub(super) async fn share_peers(&self, connection: &Connection<H>) {
        let to = connection.remote();
        let shared: Shared = self
            .peers
            .borrow()
            .keys()
            .filter(|peer| **peer != to)
            .filter_map(|peer| Some((vinc::node_to_string(peer), self.directory.get(peer)?)))
            .take(MAX_SHARED_PEERS)
            .collect();
        if shared.is_empty() {
            return;
        }
        let sent = async {
            let payload = wire::encode(&shared)?;
            let (mut send, mut recv) = connection
                .open_bi_tagged(services::STREAM_PEERS)
                .await
                .map_err(|e| wire::vinc_error("open peers stream", e))?;
            send.set_urgency(services::urgency_of(services::STREAM_PEERS))
                .await
                .map_err(|e| wire::vinc_error("peers urgency", e))?;
            wire::write_payload(&mut send, &payload).await?;
            send.finish()
                .await
                .map_err(|e| wire::vinc_error("finish peers", e))?;
            let _ =
                crate::rt::time::timeout(std::time::Duration::from_secs(5), recv.read_to_end(16))
                    .await;
            Ok::<_, crate::error::Error>(())
        };
        if let Err(error) = sent.await {
            debug!(%error, "vinc peer exchange not delivered");
        }
    }

    /// Keep the records `payload` carries and dial each peer this node is not
    /// connected to.
    pub(super) fn learn_peers(self: &Rc<Self>, payload: &[u8]) -> Result<()> {
        let shared: Shared = wire::decode(payload)?;
        for (id, record) in shared.into_iter().take(MAX_SHARED_PEERS) {
            let Ok(node) = vinc::node_from_str(&id) else {
                continue;
            };
            if self.known_or_barred(&node) {
                continue;
            }
            self.directory.insert(node, record);
            let dialer = Rc::clone(self);
            self.spawn(async move {
                if let Err(error) = dialer.connection(&node, Vec::new()).await {
                    debug!(peer_id = %node_to_peer_id(&node), %error, "vinc dial of an exchanged peer failed");
                }
            });
        }
        Ok(())
    }

    fn known_or_barred(&self, node: &NodeId) -> bool {
        *node == self.ep.id() || self.link(node).is_some() || self.ep.is_revoked(node)
    }
}
