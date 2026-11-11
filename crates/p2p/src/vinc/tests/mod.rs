//! Unit tests of the vinc transport: two or more real endpoints on loopback.
//!
//! Each test is a plain `#[test]` whose body runs on the shared test edge
//! (`workers::block_on` on its task), so the transports' timers and sockets
//! are the edge's, as in a node.

mod allowlist;
mod bench;
mod bridge;
mod identity;
mod inbound_admission;
mod mux;
mod two_stream;

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use bytes::Bytes;
use zeroize::Zeroizing;

use super::endpoint::EventReceiver;
use super::{
    spawn_endpoint, VincAllowlistConfig, VincEndpointConfig, VincResponseToken, VincTransport,
};
use crate::message::PushLogRequest;
use crate::signing::sign_with_transport;
use crate::transport::{P2PTransport, PeerId, TransportEvent};

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

/// A fresh seed: every test node has its own identity.
pub(super) fn seed() -> Zeroizing<[u8; 32]> {
    use sha2::Digest as _;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let digest = sha2::Sha256::digest(format!("vinc-test-{}-{nanos}-{n}", std::process::id()));
    Zeroizing::new(digest.into())
}

pub(super) struct TestNode {
    pub(super) id: PeerId,
    pub(super) transport: VincTransport,
    pub(super) events: EventReceiver,
    task: edge::Task<()>,
}

impl TestNode {
    pub(super) async fn spawn() -> Self {
        Self::with_allowlist(VincAllowlistConfig::AcceptAll).await
    }

    pub(super) async fn with_allowlist(allowlist: VincAllowlistConfig) -> Self {
        let mut config = VincEndpointConfig::new(seed(), loopback());
        config.allowlist = allowlist;
        config.max_connections = 8;
        Self::with_config(config).await
    }

    pub(super) async fn with_config(config: VincEndpointConfig) -> Self {
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
        Pick: FnMut(TransportEvent<VincResponseToken>) -> Option<T>,
    {
        loop {
            let event = crate::rt::time::timeout(IO_TIMEOUT, self.events.recv())
                .await
                .expect("timed out waiting for a vinc event")
                .expect("vinc event channel closed");
            if let Some(found) = pick(event) {
                return found;
            }
        }
    }
}

/// Dial `receiver` from `sender` and wait until both see the connection.
pub(super) async fn connect(sender: &VincTransport, receiver: &VincTransport) {
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

pub(super) fn signed_request(transport: &VincTransport, label: &str) -> PushLogRequest {
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
) -> (PeerId, PushLogRequest, VincResponseToken) {
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
