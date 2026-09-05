//! Every Defra protocol shares one QUIC connection per peer.
//!
//! The peer here is a bare iroh endpoint rather than a second transport, so the
//! assertions are about what a real `IrohTransport` puts on the wire: how many
//! connections it opens, and which protocol tags it multiplexes over them.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use iroh::SecretKey;
use parking_lot::Mutex;
use tokio::task::JoinHandle;

use super::protocols;
use super::{spawn_endpoint, IrohDiscoveryConfig, IrohEndpointConfig, IrohTransport};
use crate::message::{
    BranchableSyncReply, DocSyncReply, ManageMutateOp, ManageQueryOp, ManageQueryRequest,
    ManageRequest, PushSEArtifactsRequest,
};
use crate::transport::{P2PTransport, PeerAddr, PeerId};

/// What the bare peer saw: how many QUIC connections were opened to it, and the
/// protocol tag of every stream that arrived.
#[derive(Default, Clone)]
struct Observed {
    connections: usize,
    tags: Vec<Vec<u8>>,
}

impl Observed {
    fn distinct_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .tags
            .iter()
            .map(|tag| String::from_utf8_lossy(tag).into_owned())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }
}

struct BarePeer {
    peer_id: PeerId,
    addr: SocketAddr,
    observed: Arc<Mutex<Observed>>,
    accept_task: JoinHandle<()>,
}

impl BarePeer {
    fn observed(&self) -> Observed {
        self.observed.lock().clone()
    }
}

/// A peer that speaks nothing but the mux ALPN: it accepts connections, reads
/// each stream's tag, and closes its side so one-way senders do not block.
async fn spawn_bare_peer() -> BarePeer {
    let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![protocols::ALPN_MUX.to_vec()])
        .bind_addr("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .expect("bind addr")
        .bind()
        .await
        .expect("bind endpoint");

    let peer_id = PeerId::new(endpoint.id().to_string());
    let addr = endpoint
        .addr()
        .ip_addrs()
        .next()
        .copied()
        .expect("listener direct address");

    let observed = Arc::new(Mutex::new(Observed::default()));
    let accept_task = tokio::spawn({
        let endpoint = endpoint.clone();
        let observed = Arc::clone(&observed);
        async move {
            // The endpoint must outlive the accept loop, so hold it here.
            let _endpoint = endpoint.clone();
            while let Some(incoming) = endpoint.accept().await {
                let Ok(connection) = incoming.await else {
                    continue;
                };
                observed.lock().connections += 1;
                let observed = Arc::clone(&observed);
                tokio::spawn(async move {
                    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                        match protocols::read_stream_tag(&mut recv).await {
                            Ok(tag) => observed.lock().tags.push(tag),
                            Err(_) => break,
                        }
                        let _ = send.finish();
                    }
                });
            }
        }
    });

    BarePeer {
        peer_id,
        addr,
        observed,
        accept_task,
    }
}

async fn spawn_transport() -> (IrohTransport, JoinHandle<()>) {
    let secret_key = SecretKey::generate();
    let config = IrohEndpointConfig {
        secret_key: secret_key.clone(),
        node_identity: None,
        relay_mode: super::IrohRelayModeConfig::Disabled,
        discovery: IrohDiscoveryConfig::Disabled,
        bind_port: None,
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        max_concurrent_multipath_paths: None,
        gossip_heal: Default::default(),
    };
    let (command_tx, _events, _replicators, task) = spawn_endpoint(config).await.unwrap();
    (IrohTransport::new(command_tx, secret_key), task)
}

/// Drive six protocols that a client uses to interact with a node's DAG, each
/// on a different stream tag.
async fn drive_six_protocols(transport: &IrohTransport, peer: &PeerId) {
    transport
        .send_doc_sync_response(peer, DocSyncReply::success("doc-sync", Vec::new()))
        .await
        .expect("doc sync response");
    transport
        .send_branchable_sync_response(
            peer,
            BranchableSyncReply::success("branchable", "collection1", Vec::new()),
        )
        .await
        .expect("branchable sync response");
    transport
        .send_car_response(peer, vec![1, 2, 3])
        .await
        .expect("car response");
    transport
        .send_se_artifacts(peer, PushSEArtifactsRequest::new("collection1", Vec::new()))
        .await
        .expect("se artifacts");
    transport
        .send_manage_request(
            peer,
            ManageRequest::new(
                ManageMutateOp::ReplicatorDelete {
                    addresses: Vec::new(),
                    collection_ids: Vec::new(),
                },
                Vec::new(),
            ),
        )
        .await
        .expect("manage request");
    transport
        .send_manage_query_request(
            peer,
            ManageQueryRequest::new(ManageQueryOp::ReplicatorList, Vec::new()),
        )
        .await
        .expect("manage query request");
}

/// The point of the mux ALPN: six protocols, one connection.
///
/// Before multiplexing each protocol negotiated its own ALPN, so this exchange
/// cost six QUIC handshakes — six congestion controllers, six hole-punches, six
/// keepalive timers — against a single peer.
#[tokio::test]
async fn six_protocols_share_one_connection() {
    let peer = spawn_bare_peer().await;
    let (transport, endpoint_task) = spawn_transport().await;

    transport
        .dial(&peer.peer_id, vec![PeerAddr::new(peer.addr.to_string())])
        .await
        .expect("dial bare peer");

    drive_six_protocols(&transport, &peer.peer_id).await;

    let observed = peer.observed();
    assert_eq!(
        observed.distinct_tags().len(),
        6,
        "expected six protocols, saw {:?}",
        observed.distinct_tags()
    );
    assert_eq!(
        observed.connections,
        1,
        "six protocols must share one connection, saw {} (tags: {:?})",
        observed.connections,
        observed.distinct_tags()
    );

    peer.accept_task.abort();
    transport.shutdown().await.unwrap();
    endpoint_task.await.unwrap();
}

/// Repeating a protocol reuses the connection rather than redialling.
#[tokio::test]
async fn repeated_sends_do_not_redial() {
    let peer = spawn_bare_peer().await;
    let (transport, endpoint_task) = spawn_transport().await;

    transport
        .dial(&peer.peer_id, vec![PeerAddr::new(peer.addr.to_string())])
        .await
        .expect("dial bare peer");

    for _ in 0..5 {
        transport
            .send_car_response(&peer.peer_id, vec![7])
            .await
            .expect("car response");
    }

    let observed = peer.observed();
    assert_eq!(observed.tags.len(), 5, "every send must reach the peer");
    assert_eq!(
        observed.connections, 1,
        "repeated sends must reuse the dialled connection"
    );

    peer.accept_task.abort();
    transport.shutdown().await.unwrap();
    endpoint_task.await.unwrap();
}

/// A stream-level failure must not evict the connection every other protocol is
/// using. An unknown tag is dispatched as a no-op by the peer; the sends around
/// it keep flowing on the same connection.
#[tokio::test]
async fn a_failed_stream_does_not_tear_down_the_connection() {
    let peer = spawn_bare_peer().await;
    let (transport, endpoint_task) = spawn_transport().await;

    transport
        .dial(&peer.peer_id, vec![PeerAddr::new(peer.addr.to_string())])
        .await
        .expect("dial bare peer");

    transport
        .send_car_response(&peer.peer_id, vec![1])
        .await
        .expect("car response before");

    // A CAR *request* expects bytes back; the bare peer never sends any, so this
    // stream fails on read. The connection itself stays healthy.
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        transport.send_car_request(&peer.peer_id, cid_for_test()),
    )
    .await;

    transport
        .send_car_response(&peer.peer_id, vec![2])
        .await
        .expect("car response after");

    let observed = peer.observed();
    assert_eq!(
        observed.connections, 1,
        "a failed stream must not force a redial"
    );

    peer.accept_task.abort();
    transport.shutdown().await.unwrap();
    endpoint_task.await.unwrap();
}

fn cid_for_test() -> cid::Cid {
    use multihash_codetable::{Code, MultihashDigest};
    cid::Cid::new_v1(0x71, Code::Sha2_256.digest(b"mux-test"))
}
