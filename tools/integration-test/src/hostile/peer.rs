//! An in-process iroh peer that pushes whatever fragments it is handed.
//!
//! It speaks the production transport and message types, so the receiving
//! node cannot tell it from another defra node: it holds its own endpoint key,
//! signs its PushLog envelopes with it, and needs no cooperation from the node.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use cid::Cid;
use p2p::iroh::{
    generate_secret_key, spawn_endpoint, IrohDiscoveryConfig, IrohEndpointConfig,
    IrohRelayModeConfig, IrohTransport,
};
use p2p::transport::{P2PTransport, PeerId};
use p2p::{PushLogReply, PushLogRequest};

use super::blocks::Fragment;

pub struct HostilePeer {
    transport: IrohTransport,
    target: PeerId,
    _drain: edge::Service,
}

/// The most requests a silent peer keeps open; later ones are dropped, which
/// the node reads as an empty reply.
const HELD_REQUESTS: usize = 64;

impl HostilePeer {
    /// Connect to the node. A request the node sends this peer, such as a
    /// fetch for blocks it is missing, is dropped unanswered, which the node
    /// reads as an empty reply at once.
    pub async fn dial(p2p_addrs: &[String]) -> Self {
        Self::connect(p2p_addrs, false).await
    }

    /// Connect like `dial`, but keep every request the node sends this peer
    /// open and unanswered until the peer drops, so a fetch from it runs to
    /// its own timeout instead of ending on an empty reply.
    pub async fn dial_silent(p2p_addrs: &[String]) -> Self {
        Self::connect(p2p_addrs, true).await
    }

    async fn connect(p2p_addrs: &[String], hold_requests: bool) -> Self {
        let secret_key = generate_secret_key();
        let config = IrohEndpointConfig {
            secret_key: secret_key.clone(),
            relay_mode: IrohRelayModeConfig::Disabled,
            discovery: IrohDiscoveryConfig::Disabled,
            bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            ..Default::default()
        };
        let edge = edge::testing::edge();
        let (commands, mut events, _replicators, endpoint) = spawn_endpoint(config, edge.clone())
            .await
            .expect("endpoint spawns");
        // The endpoint loop ends on its own once the transport drops its command sender.
        drop(endpoint);
        let transport = IrohTransport::new(commands, secret_key);

        // The node asks the pusher for blocks it is missing. Never answering
        // is deliberate, but the queue must keep moving or the endpoint stalls.
        // A held request keeps its reply stream open until the drain stops.
        let drain = edge.spawn_service(async move {
            let mut held = Vec::new();
            while let Some(event) = events.recv().await {
                if hold_requests && held.len() < HELD_REQUESTS {
                    held.push(event);
                }
            }
        });

        let mut target = None;
        let mut addrs = Vec::new();
        for p2p_addr in p2p_addrs {
            let (peer_id, peer_addrs) = transport
                .parse_dial_addr(p2p_addr)
                .expect("node address parses");
            addrs.extend(peer_addrs);
            target = Some(peer_id);
        }
        let target = target.expect("the node reports an address");
        // A dial can time out while sibling tests saturate the machine; a real
        // peer would simply redial, and connecting is not what is under test.
        let mut attempt = 1;
        loop {
            let connected = match transport.dial(&target, addrs.clone()).await {
                Ok(()) => {
                    transport
                        .poll_until_connected(&target, Duration::from_secs(15))
                        .await
                }
                Err(error) => Err(error),
            };
            match connected {
                Ok(()) => break,
                Err(error) if attempt < 5 => {
                    eprintln!("hostile peer dial attempt {attempt} failed: {error}");
                    attempt += 1;
                }
                Err(error) => panic!("hostile peer could not connect to {p2p_addrs:?}: {error}"),
            }
        }

        Self {
            transport,
            target,
            _drain: drain,
        }
    }

    /// This peer's own ID, in the form the node's sync status reports a
    /// pushing peer as.
    pub fn peer_id(&self) -> String {
        self.transport.local_peer_id().to_string()
    }

    /// Push only `fragment`'s root, one PushLog, and return the node's reply.
    /// The node then holds the root but not the blocks it links, so it keeps
    /// a pending fetch for them until something serves the DAG.
    pub async fn push_root(&self, fragment: &Fragment, collection_id: &str) -> PushLogReply {
        let bytes = fragment
            .blocks
            .iter()
            .find(|(cid, _)| *cid == fragment.root)
            .map(|(_, bytes)| bytes.clone())
            .expect("the fragment holds its root block");
        let request = PushLogRequest::new(
            fragment.doc_id.clone(),
            fragment.root.to_bytes().into(),
            collection_id.to_string(),
            fragment.creator.clone(),
            bytes.into(),
        );
        self.transport
            .send_two_stream_request(&self.target, request)
            .await
            .expect("the node answers the push")
    }

    /// Push every block of a fragment in order, one PushLog each, and return
    /// the node's reply to each.
    pub async fn push(&self, fragment: &Fragment, collection_id: &str) -> Vec<(Cid, PushLogReply)> {
        let mut replies = Vec::with_capacity(fragment.blocks.len());
        for (cid, bytes) in &fragment.blocks {
            let request = PushLogRequest::new(
                fragment.doc_id.clone(),
                cid.to_bytes().into(),
                collection_id.to_string(),
                fragment.creator.clone(),
                bytes.clone().into(),
            );
            let reply = self
                .transport
                .send_two_stream_request(&self.target, request)
                .await
                .expect("the node answers the push");
            replies.push((*cid, reply));
        }
        replies
    }
}
