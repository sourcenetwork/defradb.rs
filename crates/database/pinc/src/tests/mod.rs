//! Unit tests of the pinc transport: two or more real endpoints on loopback.
//!
//! Each test is a plain `#[test]` whose body runs on the shared test edge
//! (`workers::block_on` on its task), so the transports' timers and sockets
//! are the edge's, as in a node.

mod allowlist;
mod bench;
mod block_sync;
mod bridge;
mod exchange;
mod identity;
mod inbound_admission;
mod mux;
mod protocol_streams;
mod redial;
mod relay;
mod sticky_disconnect;
mod topics;
mod two_stream;

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bytes::Bytes;
use zeroize::Zeroizing;

use super::endpoint::EventReceiver;
use super::{
    spawn_endpoint, spawn_hub, HubConfig, PincAllowlistConfig, PincEndpointConfig,
    PincResponseToken, PincTransport,
};
use p2p::message::PushLogRequest;
use p2p::signing::sign_with_transport;
use p2p::transport::{P2PTransport, PeerId, TransportEvent};

pub(super) const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Run `body` on the test edge and wait for it here.
pub(super) fn run<F>(body: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let edge = edge::testing::edge();
    workers::block_on(edge.spawn(body))
        .expect("the test thread waits")
        .expect("the test body runs to its end");
}

pub(super) fn loopback() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

/// A hub on loopback, with pinc's relay role when `relay`, and its ticket.
pub(super) async fn hub_with(relay: bool) -> String {
    let mut config = HubConfig::new(seed(), loopback());
    config.relay = relay;
    let (ticket, task) = spawn_hub(config, edge::testing::edge()).await.unwrap();
    // ponytail: the hub runs until the test edge closes with the process.
    std::mem::forget(task);
    ticket
}

/// A fresh seed: every test node has its own identity.
pub(super) fn seed() -> Zeroizing<[u8; 32]> {
    use sha2::Digest as _;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let digest = sha2::Sha256::digest(format!("pinc-test-{}-{nanos}-{n}", std::process::id()));
    Zeroizing::new(digest.into())
}

pub(super) struct TestNode {
    pub(super) id: PeerId,
    pub(super) transport: PincTransport,
    pub(super) events: EventReceiver,
    task: edge::Task<()>,
}

impl TestNode {
    pub(super) async fn spawn() -> Self {
        Self::with_allowlist(PincAllowlistConfig::AcceptAll).await
    }

    pub(super) async fn with_allowlist(allowlist: PincAllowlistConfig) -> Self {
        let mut config = PincEndpointConfig::new(seed(), loopback());
        config.allowlist = allowlist;
        config.max_connections = 8;
        Self::with_config(config).await
    }

    pub(super) async fn with_config(config: PincEndpointConfig) -> Self {
        let (transport, events, _replicators, task) =
            spawn_endpoint(config, edge::testing::edge()).await.unwrap();
        Self {
            id: transport.local_peer_id().clone(),
            transport,
            events,
            task,
        }
    }

    pub(super) async fn shutdown(self) {
        self.transport.shutdown().await.unwrap();
        self.task.await.unwrap();
    }

    /// The next event `pick` takes, skipping the others.
    pub(super) async fn next<T, Pick>(&mut self, mut pick: Pick) -> T
    where
        Pick: FnMut(TransportEvent<PincResponseToken>) -> Option<T>,
    {
        loop {
            let event = p2p::rt::time::timeout(IO_TIMEOUT, self.events.recv())
                .await
                .expect("timed out waiting for a pinc event")
                .expect("pinc event channel closed");
            if let Some(found) = pick(event) {
                return found;
            }
        }
    }
}

/// Dial `receiver` from `sender` and wait until both see the connection.
pub(super) async fn connect(sender: &PincTransport, receiver: &PincTransport) {
    sender
        .dial(
            receiver.local_peer_id(),
            receiver.listen_addresses().await.unwrap(),
        )
        .await
        .unwrap();
    sender
        .poll_until_connected(receiver.local_peer_id(), IO_TIMEOUT)
        .await
        .unwrap();
    receiver
        .poll_until_connected(sender.local_peer_id(), IO_TIMEOUT)
        .await
        .unwrap();
}

pub(super) fn signed_request(transport: &PincTransport, label: &str) -> PushLogRequest {
    let mut request = PushLogRequest::new(
        format!("doc-{label}"),
        Bytes::from(vec![1, 2, 3]),
        "collection1".to_string(),
        "creator1".to_string(),
        Bytes::from(format!("block-{label}").into_bytes()),
    );
    sign_with_transport(transport, &mut request).unwrap();
    request
}

/// The next two-stream request a node receives, with its token.
pub(super) async fn next_two_stream_request(
    node: &mut TestNode,
) -> (PeerId, PushLogRequest, PincResponseToken) {
    node.next(|event| match event {
        TransportEvent::TwoStreamRequest {
            peer_id,
            request,
            token: Some(token),
            ..
        } => Some((peer_id, request, token)),
        _ => None,
    })
    .await
}
