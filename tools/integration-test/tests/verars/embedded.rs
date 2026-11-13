//! A Vera node the test process hosts, reached in-process or over HTTP.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use vera::vera_rs::NativeWorker;
use vera::{AcpTuning, ProviderError, VeraRsProvider};
use vera_embedded::{Deployment, HostConfig, HostedNode};

/// The deployment the tests' workers sign for, as the daemon cluster's.
pub const DEPLOYMENT: u64 = 9001;
/// The compressed secp256k1 key of the deployment's only operator.
const OPERATOR: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
/// The actor the tests' providers act as. An embedded deployment charges
/// nothing, so any key does.
pub const ACTOR_KEY: [u8; 32] = [42; 32];

/// A single-validator Vera node in this process, with its JSON-RPC methods
/// also served on a loopback port.
pub struct Embedded {
    node: HostedNode,
    rpc: SocketAddr,
    _directory: tempfile::TempDir,
}

impl Embedded {
    pub async fn start() -> Self {
        let directory = tempfile::tempdir().expect("directory");
        // A port that was free a moment ago; the node binds it next.
        let rpc = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .expect("free port");
        let node = HostedNode::start_serving(
            HostConfig {
                data_dir: directory.path().join("vera"),
                deployment: Deployment {
                    chain_id: DEPLOYMENT,
                    threshold: 1,
                    operator_keys: vec![OPERATOR.to_string()],
                    created: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("clock")
                        .as_secs(),
                },
                listen: "127.0.0.1:0".parse().expect("address"),
                start_deadline: Duration::from_secs(60),
                stop_deadline: Duration::from_secs(30),
            },
            Some(rpc),
            &edge::testing::edge(),
        )
        .await
        .expect("embedded Vera node");
        Self {
            node,
            rpc,
            _directory: directory,
        }
    }

    /// The node's HTTP JSON-RPC address.
    pub fn rpc_url(&self) -> String {
        format!("http://{}", self.rpc)
    }

    /// A provider that reaches the node through its in-process method table.
    pub async fn in_process(&self, worker: NativeWorker) -> Result<VeraRsProvider, ProviderError> {
        let tuning = AcpTuning::default();
        let transport = Arc::new(self.node.transport(tuning.request_timeout));
        VeraRsProvider::with_transport(
            transport.clone(),
            transport,
            self.node.consensus_key(),
            &ACTOR_KEY,
            worker,
            &tuning,
            None,
            &edge::testing::edge(),
        )
        .await
    }

    /// A provider that reaches the same node over HTTP and a WebSocket.
    pub async fn over_http(&self, worker: NativeWorker) -> Result<VeraRsProvider, ProviderError> {
        VeraRsProvider::new(
            self.rpc_url(),
            self.node.consensus_key(),
            &ACTOR_KEY,
            worker,
            &AcpTuning::default(),
            None,
            &edge::testing::edge(),
        )
        .await
    }

    pub async fn stop(self) {
        self.node.stop().await.expect("stop");
    }
}

/// A worker with a journal and keyring of its own under `root`.
pub fn worker(root: &std::path::Path) -> NativeWorker {
    let keyring = keyring::FileKeyring::open(root.join("keys"), b"test-password").expect("keyring");
    NativeWorker::open(&root.join("worker"), &keyring, DEPLOYMENT).expect("worker")
}
