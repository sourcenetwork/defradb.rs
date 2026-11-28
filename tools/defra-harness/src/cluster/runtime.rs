use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::error::{Result, WrapErr};
use reqwest::Client;

use crate::client::DefraClient;
use crate::divergences::NodeKind;
use crate::node::{start_node, DefraNode, NodeConfig, RunningNode, RustNode};
use crate::observe::LogTracker;
use crate::ports::{multiaddr_ports, reserve_ports, ReservedPorts};

use super::health::health_check;

/// Attempts to bring a restarting node back up on its original ports.
///
/// Guarding the ports covers the kill → respawn window; what is left is the
/// guard-release → child-bind window, i.e. however long the node takes to boot
/// — the same exposure a fresh start has. Recovery there means waiting for a
/// *transient* holder to let go and trying the same ports again, since a
/// restart cannot move to fresh ports. Raising this number buys nothing: a
/// competitor holding the port for its own lifetime defeats any number of
/// attempts, and only re-addressing the node could recover from it.
const PORT_CONFLICT_ATTEMPTS: usize = 3;

/// How long to wait for a killed node's ports to become bindable again.
/// Its `Drop` already reaped the child, so this normally succeeds first try;
/// the budget covers a loaded host where the kernel is slower to tear the
/// listening sockets down.
const PORT_RECLAIM_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll interval while waiting for the old process's ports to free up.
const PORT_RECLAIM_POLL: Duration = Duration::from_millis(10);

/// Settle time between killing a node and respawning it. Held *under* the port
/// guards, so the node's address is never unowned while it elapses.
const RESTART_SETTLE: Duration = Duration::from_millis(200);

/// The ports a restarting node must come back on: its HTTP API, plus any
/// explicitly configured libp2p listen addresses.
///
/// Iroh nodes carry no `p2p_addr` — that transport picks its own UDP port and
/// peers address the node by NodeId — so only the API port is pinned there.
///
/// Port 0 is dropped: it asks the OS for any port, so there is nothing to pin.
fn pinned_ports(config: &NodeConfig) -> (Vec<u16>, Vec<u16>) {
    let mut tcp = Vec::new();
    let mut udp = Vec::new();
    if let Some(port) = config
        .http_addr
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
    {
        tcp.push(port);
    }
    if let Some(addr) = config.p2p_addr.as_deref() {
        let (p2p_tcp, p2p_udp) = multiaddr_ports(addr);
        tcp.extend(p2p_tcp);
        udp.extend(p2p_udp);
    }
    tcp.retain(|p| *p != 0);
    udp.retain(|p| *p != 0);
    (tcp, udp)
}

/// Reserve `tcp`/`udp` for `name`, retrying until they are free or the budget
/// runs out. The first failures are expected — the previous owner may still be
/// closing its sockets — so only the last one is reported.
async fn reserve_until_free(
    name: &str,
    tcp: &[u16],
    udp: &[u16],
    budget: Duration,
) -> Result<ReservedPorts> {
    let deadline = Instant::now() + budget;
    loop {
        match reserve_ports(tcp, udp) {
            Ok(reserved) => return Ok(reserved),
            Err(e) if Instant::now() >= deadline => {
                return Err(e).wrap_err_with(|| {
                    format!(
                        "{}: could not reclaim its ports within {:?} — a restart cannot move to \
                         fresh ports, since peers and clients hold this address",
                        name, budget
                    )
                });
            }
            Err(_) => tokio::time::sleep(PORT_RECLAIM_POLL).await,
        }
    }
}

/// A cluster of running DefraDB nodes.
///
/// Field order matters: `nodes` are dropped before `run_dir`,
/// ensuring processes are killed before their data directories are removed.
/// A node stopped by [`TestCluster::stop_node`]: everything needed to start
/// it again on the same ports, which stay reserved meanwhile.
pub struct StoppedNode {
    index: usize,
    config: NodeConfig,
    kind: NodeKind,
    name: String,
    api_url: String,
    binary_path: PathBuf,
    tcp_ports: Vec<u16>,
    udp_ports: Vec<u16>,
    reserved: ReservedPorts,
}

pub struct TestCluster {
    pub nodes: Vec<RunningNode>,
    #[allow(dead_code)]
    run_dir: crate::infra::TestRunDir,
    startup_identity: Option<String>,
    node_identities: Vec<Option<String>>,
}

impl TestCluster {
    pub(crate) fn new(
        nodes: Vec<RunningNode>,
        run_dir: crate::infra::TestRunDir,
        startup_identity: Option<String>,
        node_identities: Vec<Option<String>>,
    ) -> Self {
        Self {
            nodes,
            run_dir,
            startup_identity,
            node_identities,
        }
    }

    /// Return the private key hex used to start nodes (if any).
    ///
    /// In NAC mode, Go grants automatic admin access to the startup identity.
    /// Tests must use this identity for admin operations.
    pub fn startup_identity(&self) -> Option<&str> {
        self.startup_identity.as_deref()
    }

    /// Return the identity for a specific node (if set via per-node override).
    pub fn node_identity(&self, index: usize) -> Option<&str> {
        self.node_identities.get(index).and_then(|id| id.as_deref())
    }

    pub fn builder() -> super::builder::TestClusterBuilder {
        super::builder::TestClusterBuilder::new()
    }

    /// Return a CLI-based client for the node at `index`.
    pub fn client(&self, index: usize) -> DefraClient {
        let node = &self.nodes[index];
        DefraClient::new(&node.binary_path, &node.http_addr, node.kind)
    }

    pub fn api_url(&self, index: usize) -> &str {
        &self.nodes[index].api_url
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Wait for a named log pattern on the node at `index`.
    pub async fn wait_for_log(
        &self,
        index: usize,
        pattern: &str,
        timeout: Duration,
    ) -> Result<String> {
        self.nodes[index]
            .log_tracker
            .wait_for_pattern(pattern, timeout)
            .await
    }

    /// Stop a node with the SIGTERM path and hold its ports so nothing can be
    /// handed the address while it is down. Start it again on the same ports
    /// with [`start_stopped_node`](Self::start_stopped_node).
    pub async fn stop_node(&mut self, index: usize) -> Result<StoppedNode> {
        // A paused node holds SIGTERM until it runs again, so it would be
        // killed instead of stopping; let it run first.
        let _ = self.resume_node(index);
        let old = &self.nodes[index];
        let config = old.config.clone();
        let kind = old.kind;
        let name = old.name.clone();
        let api_url = old.api_url.clone();
        let binary_path = old.binary_path.clone();
        let (tcp_ports, udp_ports) = pinned_ports(&config);

        // Drop old node to kill the process
        let old_node = std::mem::replace(
            &mut self.nodes[index],
            // Placeholder — will be overwritten below
            RunningNode {
                name: String::new(),
                api_url: String::new(),
                http_addr: String::new(),
                binary_path: PathBuf::new(),
                process: crate::infra::ManagedProcess::empty(),
                log_tracker: LogTracker::empty(),
                rootdir: PathBuf::new(),
                config: config.clone(),
                kind,
            },
        );
        drop(old_node);

        // Take the ports back before anything else can be handed them.
        let reserved =
            reserve_until_free(&name, &tcp_ports, &udp_ports, PORT_RECLAIM_TIMEOUT).await?;
        Ok(StoppedNode {
            index,
            config,
            kind,
            name,
            api_url,
            binary_path,
            tcp_ports,
            udp_ports,
            reserved,
        })
    }

    /// Start a node stopped by [`stop_node`](Self::stop_node) on its own
    /// ports; losing the boot-time bind race is retried on the same ports.
    pub async fn start_stopped_node(
        &mut self,
        stopped: StoppedNode,
        timeout: Duration,
    ) -> Result<()> {
        let StoppedNode {
            index,
            config,
            kind,
            name,
            api_url,
            binary_path,
            tcp_ports,
            udp_ports,
            mut reserved,
        } = stopped;
        let node: Box<dyn DefraNode> = match kind {
            // Respawn from the node's configured binary (e.g. a release artifact
            // or a downloaded version), not the default debug workspace path —
            // otherwise restart breaks whenever the node was not built via
            // `from_workspace()`.
            NodeKind::Rust => Box::new(RustNode::from_binary(binary_path.clone())),
            NodeKind::Go => Box::new(crate::node::GoNode::from_binary(binary_path.clone())),
        };

        let mut attempt = 1;
        let running = loop {
            // Release the guards immediately before spawn so the child can bind.
            reserved.release();
            match start_node(node.as_ref(), config.clone(), timeout).await {
                Ok(r) => break r,
                Err(e) if attempt < PORT_CONFLICT_ATTEMPTS && e.is_port_conflict() => {
                    attempt += 1;
                    tracing::warn!(
                        "{}: port stolen in the restart guard-release window; retrying on the \
                         same ports (attempt {}/{})",
                        name,
                        attempt,
                        PORT_CONFLICT_ATTEMPTS
                    );
                    reserved =
                        reserve_until_free(&name, &tcp_ports, &udp_ports, PORT_RECLAIM_TIMEOUT)
                            .await?;
                }
                Err(e) => {
                    return Err(e).wrap_err_with(|| format!("{}: failed to restart", name));
                }
            }
        };

        let client = Client::new();
        health_check(&client, &api_url, timeout)
            .await
            .wrap_err_with(|| format!("{}: health check failed after restart", name))?;

        self.nodes[index] = running;

        Ok(())
    }

    /// Restart a node on the same ports: stop, settle while the ports are
    /// still guarded, start.
    ///
    /// The node's ports are re-reserved the moment the old process releases
    /// them and held until the replacement is spawned, so nothing can be handed
    /// the address while the node is down; losing the remaining boot-time race
    /// is retried on the same ports. Fresh ports are not an option — the
    /// caller's clients and the node's peers both hold this address.
    pub async fn restart_node(&mut self, index: usize, timeout: Duration) -> Result<()> {
        let stopped = self.stop_node(index).await?;
        tokio::time::sleep(RESTART_SETTLE).await;
        self.start_stopped_node(stopped, timeout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(http_addr: &str, p2p_addr: Option<&str>) -> NodeConfig {
        let mut config = NodeConfig::new(
            "test",
            PathBuf::from("/tmp/defra-harness-runtime-test/data"),
            PathBuf::from("/tmp/defra-harness-runtime-test/logs"),
            http_addr,
        );
        config.p2p_addr = p2p_addr.map(str::to_string);
        config
    }

    #[test]
    fn pinned_ports_covers_the_api_and_every_libp2p_listen_addr() {
        let (tcp, udp) = pinned_ports(&config_with(
            "127.0.0.1:9181",
            Some("/ip4/127.0.0.1/tcp/9171,/ip4/127.0.0.1/udp/9172/quic-v1"),
        ));
        assert_eq!(tcp, vec![9181, 9171]);
        assert_eq!(udp, vec![9172]);
    }

    #[test]
    fn pinned_ports_of_an_iroh_node_is_just_the_api() {
        let (tcp, udp) = pinned_ports(&config_with("127.0.0.1:9181", None));
        assert_eq!(tcp, vec![9181]);
        assert!(udp.is_empty());
    }

    #[test]
    fn pinned_ports_ignores_ask_the_os_ports() {
        let (tcp, udp) = pinned_ports(&config_with("127.0.0.1:0", Some("/ip4/127.0.0.1/tcp/0")));
        assert!(tcp.is_empty());
        assert!(udp.is_empty());
    }
}
