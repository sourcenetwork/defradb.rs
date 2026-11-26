//! Peer exchange: a node hands a new peer its other peers, and the peer dials
//! them, so peers of peers connect, as libp2p's Kademlia and iroh's gossip
//! connect them.
//!
//! Each peer goes with the address its connection runs to, and with the signed
//! record it sent on that connection once it sent one. A node that names hubs
//! signs its record only once pinc settles it on a relay, seconds after it
//! connects, so the address is what lets peers meet at once. Neither is
//! trusted: a dial authenticates the peer's id in its handshake, and pinc
//! verifies a record before a dial uses it, so a forged address or record
//! fails that dial and nothing else.

use std::net::SocketAddr;
use std::rc::Rc;

use pinc::{Connection, Hosted, NodeId};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::error::Result;

use super::addr::node_to_peer_id;
use super::endpoint::Node;
use super::services::{self, MAX_SHARED_PEERS};
use super::wire;

/// One peer of a peer exchange.
#[derive(Serialize, Deserialize)]
struct SharedPeer {
    id: String,
    /// The signed record the peer sent, once it sent one.
    record: Option<Vec<u8>>,
    /// The address the sharing node's connection to the peer runs to.
    addr: Option<SocketAddr>,
}

impl<H: Hosted + 'static> Node<H> {
    /// Give the peer of `connection` this node's other peers.
    pub(super) async fn share_peers(&self, connection: &Connection<H>) {
        let to = connection.remote();
        let others: Vec<(NodeId, Connection<H>)> = self
            .peers
            .borrow()
            .iter()
            .filter(|(peer, _)| **peer != to)
            .filter_map(|(peer, links)| Some((*peer, links.first()?.connection.clone())))
            .take(MAX_SHARED_PEERS)
            .collect();
        let mut shared = Vec::with_capacity(others.len());
        for (peer, link) in others {
            let addr = link
                .stats()
                .await
                .ok()
                .and_then(|stats| stats.route)
                .map(|(_, remote)| remote);
            let record = self.directory.get(&peer);
            if addr.is_some() || record.is_some() {
                shared.push(SharedPeer {
                    id: pinc::node_to_string(&peer),
                    record,
                    addr,
                });
            }
        }
        if shared.is_empty() {
            return;
        }
        let sent = async {
            let payload = wire::encode(&shared)?;
            let (mut send, mut recv) = connection
                .open_bi_tagged(services::STREAM_PEERS)
                .await
                .map_err(|e| wire::pinc_error("open peers stream", e))?;
            send.set_urgency(services::urgency_of(services::STREAM_PEERS))
                .await
                .map_err(|e| wire::pinc_error("peers urgency", e))?;
            wire::write_payload(&mut send, &payload).await?;
            send.finish()
                .await
                .map_err(|e| wire::pinc_error("finish peers", e))?;
            let _ =
                crate::rt::time::timeout(std::time::Duration::from_secs(5), recv.read_to_end(16))
                    .await;
            Ok::<_, crate::error::Error>(())
        };
        if let Err(error) = sent.await {
            debug!(%error, "pinc peer exchange not delivered");
        }
    }

    /// Keep the records `payload` carries and dial each peer this node is not
    /// connected to.
    pub(super) fn learn_peers(self: &Rc<Self>, payload: &[u8]) -> Result<()> {
        let shared: Vec<SharedPeer> = wire::decode(payload)?;
        for peer in shared.into_iter().take(MAX_SHARED_PEERS) {
            let Ok(node) = pinc::node_from_str(&peer.id) else {
                continue;
            };
            if self.known_or_barred(&node) {
                continue;
            }
            if let Some(record) = peer.record {
                self.directory.insert(node, record);
            }
            let direct = peer.addr.into_iter().collect();
            let dialer = Rc::clone(self);
            self.spawn(async move {
                // A hang-up can land after this list was read and before the
                // dial does: the quiet is read again at each end of the dial,
                // and a dial that landed on a peer hung up on meanwhile is
                // hung up on again.
                if dialer.is_quiet(&node) {
                    return;
                }
                match dialer.connection(&node, direct).await {
                    Ok(_) if dialer.is_quiet(&node) => dialer.hang_up(&node).await,
                    Ok(_) => {}
                    Err(error) => {
                        debug!(peer_id = %node_to_peer_id(&node), %error, "pinc dial of an exchanged peer failed");
                    }
                }
            });
        }
        Ok(())
    }

    fn known_or_barred(&self, node: &NodeId) -> bool {
        *node == self.ep.id()
            || self.link(node).is_some()
            || self.ep.is_revoked(node)
            || self.is_quiet(node)
    }
}
