//! Two endpoints that know each other only by id and relay URL connect
//! through an in-process relay, and the relay's allowlist gates that.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use iroh::SecretKey;
use n0_future::task::JoinHandle;

use super::{
    spawn_endpoint, IrohDiscoveryConfig, IrohEndpointConfig, IrohRelayModeConfig, IrohRelayServer,
    IrohRelayServerConfig, IrohTransport,
};
use crate::transport::{P2PTransport, PeerAddr};

const RELAYED_DIAL_TIMEOUT: Duration = Duration::from_secs(20);
const DENIED_DIAL_TIMEOUT: Duration = Duration::from_secs(5);

struct TestNode {
    transport: IrohTransport,
    task: JoinHandle<()>,
}

impl TestNode {
    async fn spawn(secret_key: SecretKey, relay_url: &str) -> Self {
        let config = IrohEndpointConfig {
            secret_key: secret_key.clone(),
            relay_mode: IrohRelayModeConfig::Custom(vec![relay_url.to_string()]),
            discovery: IrohDiscoveryConfig::Disabled,
            bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            ..Default::default()
        };
        let (command_tx, _events, _replicators, task) = spawn_endpoint(config).await.unwrap();
        Self {
            transport: IrohTransport::new(command_tx, secret_key),
            task,
        }
    }

    async fn dial_via_relay(&self, target: &TestNode, relay_url: &str) -> bool {
        let dial = self.transport.dial(
            target.transport.local_peer_id(),
            vec![PeerAddr::new(relay_url.to_string())],
        );
        matches!(
            n0_future::time::timeout(RELAYED_DIAL_TIMEOUT, dial).await,
            Ok(Ok(()))
        )
    }

    async fn shutdown(self) {
        self.transport.shutdown().await.unwrap();
        self.task.await.unwrap();
    }
}

async fn spawn_relay(allowed_endpoints: Vec<String>) -> (IrohRelayServer, String) {
    let relay = IrohRelayServer::spawn(IrohRelayServerConfig {
        http_bind_addr: "127.0.0.1:0".parse().unwrap(),
        tls: None,
        quic_bind_addr: None,
        allowed_endpoints,
        client_rx_bytes_per_second: None,
        client_rx_max_burst_bytes: None,
    })
    .await
    .unwrap();
    let url = format!("http://{}", relay.http_addr().unwrap());
    (relay, url)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn endpoints_connect_through_hosted_relay_without_direct_addresses() {
    let (relay, relay_url) = spawn_relay(Vec::new()).await;
    let dialer = TestNode::spawn(SecretKey::generate(), &relay_url).await;
    let target = TestNode::spawn(SecretKey::generate(), &relay_url).await;

    assert!(
        dialer.dial_via_relay(&target, &relay_url).await,
        "dial carrying only a relay URL must connect through the hosted relay"
    );

    dialer.shutdown().await;
    target.shutdown().await;
    relay.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relay_allowlist_refuses_unlisted_endpoints() {
    let target_key = SecretKey::generate();
    let (relay, relay_url) = spawn_relay(vec![target_key.public().to_string()]).await;
    let outsider = TestNode::spawn(SecretKey::generate(), &relay_url).await;
    let target = TestNode::spawn(target_key, &relay_url).await;

    let dial = outsider.transport.dial(
        target.transport.local_peer_id(),
        vec![PeerAddr::new(relay_url.clone())],
    );
    let connected = matches!(
        n0_future::time::timeout(DENIED_DIAL_TIMEOUT, dial).await,
        Ok(Ok(()))
    );
    assert!(
        !connected,
        "an endpoint missing from the relay allowlist must not be relayed"
    );

    outsider.shutdown().await;
    target.shutdown().await;
    relay.shutdown().await.unwrap();
}
