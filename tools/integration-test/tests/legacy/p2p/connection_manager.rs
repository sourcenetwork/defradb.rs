use std::path::{Path, PathBuf};
use std::time::Duration;

use integration_test::node::{
    start_node, DefraNode, KeyringBackend, NodeConfig, RunningNode, RustNode,
};
use integration_test::{poll_until, rust_binary, DefraClient, NodeKind};
use p2p::host::CONNECTION_PRUNE_INTERVAL;

const NODE_COUNT: usize = 5;
const LOW_WATER: u32 = 2;
const HIGH_WATER: u32 = 3;
const GRACE_PERIOD_MS: u64 = 200;
/// A prune runs on the swarm event that crosses high water, or at the latest
/// on the next periodic tick once the grace period has passed.
const PRUNE_DEADLINE: Duration =
    Duration::from_millis(GRACE_PERIOD_MS + 2 * CONNECTION_PRUNE_INTERVAL.as_millis() as u64);
const READY_TIMEOUT: Duration = Duration::from_secs(30);
const P2P_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// The line the HTTP server logs with the address it bound (`server.rs`).
const HTTP_READY: &str = "Providing HTTP API at http://";
/// The line the host logs once a peer's last connection closes (`swarm.rs`).
const PEER_DISCONNECTED: &str = "Peer disconnected";

struct RustNodeWithStartArgs {
    binary_path: PathBuf,
    extra_start_args: Vec<String>,
}

impl RustNodeWithStartArgs {
    fn new(binary_path: PathBuf, extra_start_args: Vec<String>) -> Self {
        Self {
            binary_path,
            extra_start_args,
        }
    }
}

impl DefraNode for RustNodeWithStartArgs {
    fn kind(&self) -> NodeKind {
        NodeKind::Rust
    }

    fn command_parts(&self, config: &NodeConfig) -> (PathBuf, Vec<String>, Vec<(String, String)>) {
        let node = RustNode::from_binary(self.binary_path.clone());
        let (program, mut args, envs) = node.command_parts(config);
        args.extend(self.extra_start_args.clone());
        (program, args, envs)
    }

    fn binary_path(&self) -> &Path {
        &self.binary_path
    }
}

fn client(node: &RunningNode) -> DefraClient {
    DefraClient::new(
        node.binary_path.clone(),
        node.http_addr.clone(),
        NodeKind::Rust,
    )
}

fn first_p2p_addr(client: &DefraClient) -> String {
    client
        .p2p_info()
        .expect("p2p_info")
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|value| value.as_str())
        .expect("node has no P2P address")
        .to_string()
}

fn peer_id_from_addr(addr: &str) -> String {
    addr.rsplit_once("/p2p/")
        .map_or(addr, |(_, peer_id)| peer_id)
        .to_string()
}

fn active_peer_ids(client: &DefraClient) -> Vec<String> {
    client
        .p2p_active_peers()
        .expect("p2p_active_peers")
        .as_array()
        .expect("active_peers not array")
        .iter()
        .map(|value| peer_id_from_addr(value.as_str().expect("active peer must be a string")))
        .collect()
}

async fn wait_for_active_peer(client: &DefraClient, peer_id: &str) {
    poll_until(
        || active_peer_ids(client).iter().any(|id| id == peer_id),
        P2P_TIMEOUT,
        POLL_INTERVAL,
        "peer did not appear in active_peers",
    )
    .await;
}

fn node_log(node: &RunningNode) -> String {
    std::fs::read_to_string(node.rootdir.with_file_name("logs").join("stdout.log"))
        .unwrap_or_else(|error| panic!("{} log: {error}", node.name))
}

/// The address the node's HTTP server bound, from the line it logs once it
/// listens.
fn bound_http_addr(node: &RunningNode) -> String {
    let log = node_log(node);
    let rest = &log[log
        .find(HTTP_READY)
        .unwrap_or_else(|| panic!("{} logged no HTTP address", node.name))
        + HTTP_READY.len()..];
    rest.split_whitespace()
        .next()
        .expect("HTTP address")
        .to_string()
}

/// Peers whose last connection the node closed after byte `since` of its
/// log, as the log records them.
fn disconnected_peers(node: &RunningNode, since: usize, peer_ids: &[String]) -> Vec<String> {
    let log = node_log(node);
    peer_ids
        .iter()
        .filter(|peer_id| {
            log[since..]
                .lines()
                .any(|line| line.contains(PEER_DISCONNECTED) && line.contains(peer_id.as_str()))
        })
        .cloned()
        .collect()
}

async fn start_pruning_cluster() -> (tempfile::TempDir, Vec<RunningNode>) {
    let binary_path = rust_binary();
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let mut nodes = Vec::with_capacity(NODE_COUNT);

    // Node 0 is the constrained target; nodes 1..=4 are peer dialers. Every
    // node binds port 0 and reports what it got, so no port is picked for it
    // that another process can take before it binds (#2798, #2932).
    for index in 0..NODE_COUNT {
        let name = format!("rust-connmgr-{index}");
        let rootdir = temp_dir.path().join(format!("node-{index}/data"));
        let log_dir = temp_dir.path().join(format!("node-{index}/logs"));
        let mut config = NodeConfig::new(name.clone(), rootdir, log_dir, "127.0.0.1:0");
        config.p2p_enabled = true;
        config.p2p_addr = Some("/ip4/127.0.0.1/tcp/0".to_string());
        config.keyring = KeyringBackend::None;

        let extra_start_args = if index == 0 {
            vec![
                "--connection-manager-low-water".to_string(),
                LOW_WATER.to_string(),
                "--connection-manager-high-water".to_string(),
                HIGH_WATER.to_string(),
                "--connection-manager-grace-period-ms".to_string(),
                GRACE_PERIOD_MS.to_string(),
            ]
        } else {
            Vec::new()
        };

        let node = RustNodeWithStartArgs::new(binary_path.clone(), extra_start_args);
        let mut running = start_node(&node, config, READY_TIMEOUT)
            .await
            .unwrap_or_else(|error| panic!("failed to start {name}: {error:?}"));
        running.http_addr = bound_http_addr(&running);
        nodes.push(running);
    }

    for node in &nodes {
        node.log_tracker
            .wait_for_pattern("p2p_listening", P2P_TIMEOUT)
            .await
            .unwrap_or_else(|error| panic!("{} P2P listener did not start: {error}", node.name));
    }

    (temp_dir, nodes)
}

#[nuclei::test]
async fn rust_connection_manager_prunes_to_low_water() {
    edge::compat(async {
        let (_temp_dir, nodes) = start_pruning_cluster().await;
        let clients = nodes.iter().map(client).collect::<Vec<_>>();
        let target = &clients[0];
        let target_addr = first_p2p_addr(target);
        let peer_addrs = clients[1..].iter().map(first_p2p_addr).collect::<Vec<_>>();
        let peer_ids = peer_addrs
            .iter()
            .map(|addr| peer_id_from_addr(addr))
            .collect::<Vec<_>>();

        for (index, (peer_client, peer_id)) in clients[1..4].iter().zip(peer_ids.iter()).enumerate()
        {
            peer_client
                .p2p_connect(&[&target_addr])
                .unwrap_or_else(|error| panic!("peer {} failed to connect: {error}", index + 1));
            wait_for_active_peer(target, peer_id).await;
            nuclei::time::sleep(Duration::from_millis(GRACE_PERIOD_MS + 75)).await;
        }

        let active_before_prune = active_peer_ids(target);
        assert_eq!(
            active_before_prune.len(),
            HIGH_WATER as usize,
            "expected target to have exactly the high-water number of peers before pruning"
        );
        assert!(
            peer_ids
                .iter()
                .take(3)
                .all(|peer_id| active_before_prune.contains(peer_id)),
            "expected first three peers to be active before pruning: {active_before_prune:?}"
        );

        let log_before_prune = node_log(&nodes[0]).len();
        clients[4]
            .p2p_connect(&[&target_addr])
            .expect("fourth peer failed to connect");

        // Four peers against a high water of three: the trim sheds the two
        // oldest peers, 1 and 2, down to low water. A snapshot of active peers
        // cannot show it: the target's Kademlia bootstraps half a second after
        // peer 4 enters its routing table and redials them, so the count may
        // be back at high water before the first poll. The disconnects the
        // target logged are the trim itself.
        let pruned = &peer_ids[..(NODE_COUNT - 1) - LOW_WATER as usize];
        poll_until(
            || disconnected_peers(&nodes[0], log_before_prune, pruned).len() == pruned.len(),
            PRUNE_DEADLINE,
            POLL_INTERVAL,
            "connection manager did not prune the oldest peers to low water",
        )
        .await;
    })
    .await;
}
