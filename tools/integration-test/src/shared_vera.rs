//! The one Vera node every node of a remote-DAC scenario shares, as Go's
//! harness shares one Vera service (`setupRemoteDAC`) between the nodes of a
//! test run with remote document ACP.
//!
//! The node runs in the test process (`vera_embedded::HostedNode`) and serves
//! its JSON-RPC methods on a loopback port; each DefraDB node reaches it there
//! through vera.rs document ACP and verifies every read against its consensus
//! key, so a policy, document or relationship one node writes is what every
//! other node decides on.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tempfile::TempDir;
use vera_embedded::{Deployment, HostConfig, HostedNode};

/// The deployment every scenario's Vera node runs.
const DEPLOYMENT: u64 = 9001;
/// The compressed secp256k1 key of the deployment's only operator.
const OPERATOR: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
/// How many times a start is tried on a fresh directory before it fails.
const START_ATTEMPTS: usize = 3;
/// How long a client identity's delegation holds: longer than any scenario.
const DELEGATION_LIFETIME: Duration = Duration::from_secs(15 * 60);

/// The shared Vera node a DefraDB node decides document ACP on, and the
/// directory its Vera worker keeps its journal and key in across restarts.
pub type NodeVera = (Arc<SharedVera>, Arc<TempDir>);

/// A running Vera node, the address it serves JSON-RPC on, and its state.
pub struct SharedVera {
    node: Option<HostedNode>,
    rpc: SocketAddr,
    _directory: TempDir,
}

impl SharedVera {
    /// Starts the node, on a fresh directory each try.
    pub fn start(edge: &edge::Edge) -> Result<Self, String> {
        let mut last = String::new();
        for _ in 0..START_ATTEMPTS {
            match workers::block_on(Self::start_once(edge)) {
                Ok(Ok(vera)) => return Ok(vera),
                Ok(Err(error)) => last = error,
                Err(error) => last = error.to_string(),
            }
        }
        Err(format!("the shared Vera node did not start: {last}"))
    }

    async fn start_once(edge: &edge::Edge) -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(|e| format!("directory: {e}"))?;
        let node = HostedNode::start_serving(
            HostConfig {
                data_dir: directory.path().join("vera"),
                deployment: Deployment {
                    chain_id: DEPLOYMENT,
                    threshold: 1,
                    operator_keys: vec![OPERATOR.to_owned()],
                    created: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| format!("clock: {e}"))?
                        .as_secs(),
                    policies: Vec::new(),
                },
                listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
                start_deadline: Duration::from_secs(60),
                stop_deadline: Duration::from_secs(30),
            },
            Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
            edge,
        )
        .await
        .map_err(|e| e.to_string())?;
        let rpc = node
            .rpc_addr()
            .ok_or("the Vera node bound no JSON-RPC listener")?;
        Ok(Self {
            node: Some(node),
            rpc,
            _directory: directory,
        })
    }

    /// The configuration a DefraDB node acting as `actor_key`, a secp256k1
    /// key, reaches this Vera node with. Its worker keeps its journal and key
    /// under `home`, which must outlive the node and survive its restarts.
    pub fn config(
        &self,
        actor_key: &[u8],
        home: &Path,
    ) -> Result<defra_node::VeraRsConfig, String> {
        let node = self.node.as_ref().ok_or("the shared Vera node stopped")?;
        let keyring: Arc<dyn keyring::Keyring> = Arc::new(
            keyring::FileKeyring::open(home.join("keys"), b"test-password")
                .map_err(|e| format!("Vera worker keyring: {e}"))?,
        );
        Ok(defra_node::VeraRsConfig {
            rpc_address: format!("http://{}", self.rpc),
            consensus_key: node.consensus_key().to_owned(),
            genesis_id: None,
            deployment_id: node.chain_id(),
            actor: defra_node::VeraRsActor::Secp256k1Key(actor_key.to_vec()),
            worker_dir: home.join("worker"),
            keyring,
            tuning: vera::AcpTuning::default(),
        })
    }
}

/// The Vera delegation client identity `seed` presents to `node`, `None` when
/// the node decides document ACP on no Vera node.
///
/// A vera.rs node acts on Vera for a caller only under the caller's own
/// delegation to the node's worker, as Go's remote ACP acts only under the
/// caller's bearer token: the caller signs it with its key for the audience
/// the node names, and sends it as its bearer token over HTTP.
pub fn delegation(node: &defra_node::Node, seed: u64) -> Result<Option<String>, String> {
    use acp::RemotePolicies as _;
    let Some(vera) = node.vera_acp() else {
        return Ok(None);
    };
    let audience = vera
        .light_client_status()?
        .delegation
        .ok_or("the node's Vera names no delegation audience")?;
    let key = crate::scenario::identity::private_key(seed);
    let signer = vera::vera_rs::Secp256k1Signer::from_slice(&key).map_err(|e| e.to_string())?;
    workers::block_on(vera::vera_rs::caller_delegation(
        &signer,
        &audience,
        DELEGATION_LIFETIME,
    ))
    .map_err(|e| e.to_string())?
    .map(Some)
    .map_err(|e| e.to_string())
}

impl Drop for SharedVera {
    fn drop(&mut self) {
        if let Some(node) = self.node.take() {
            if let Ok(Err(error)) = workers::block_on(node.stop()) {
                eprintln!("the shared Vera node did not stop: {error}");
            }
        }
    }
}
