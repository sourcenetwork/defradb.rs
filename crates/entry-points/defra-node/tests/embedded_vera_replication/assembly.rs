//! How a peer's node is built: its store, keys, transport and Vera
//! deployment.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use defra_node::{
    EmbeddedVeraActor, EmbeddedVeraConfig, EmbeddedVeraDeployment, EmbeddedVeraPolicy,
    EmbeddedVeraTrust, Node, P2PConfig, PeerKeySource, ReplicaGrant, ReplicaOwnership,
    ReplicaRegistration, SigningKey, VeraRsActor, VeraRsConfig,
};
use identity::Did;
use vera::vera_rs::{DelegationSigner, Ed25519Signer};
use zanzibar::types::Subject;

use super::peer::{Blocks, PeerSpec};

/// The compressed secp256k1 key of the account that administers every
/// deployment.
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

pub(crate) const POLICY: &str = "\
name: replicated
resources:
  - name: users
    permissions:
      - name: read
        expr: writer + reader
      - name: update
        expr: writer
      - name: delete
        expr: writer
    relations:
      - name: writer
        types:
          - actor
      - name: reader
        types:
          - actor
";

/// A deployment whose own actors are `own`: a document anyone else created
/// is registered by the receiving node, which grants its creator `writer`.
struct Deployment {
    own: Vec<Did>,
}

impl ReplicaOwnership for Deployment {
    fn is_own_actor(&self, _collection: &str, actor: &Did) -> bool {
        self.own.contains(actor)
    }

    fn creator_grants(&self, _collection: &str, creator: &Did, _doc_id: &str) -> Vec<ReplicaGrant> {
        vec![ReplicaGrant::new(
            "writer",
            Subject::Entity(creator.clone()),
        )]
    }
}

fn keyring(spec: &PeerSpec) -> Arc<dyn keyring::Keyring> {
    Arc::new(keyring::FileKeyring::open(spec.home.join("keys"), b"test-password").expect("keyring"))
}

fn signer(spec: &PeerSpec) -> Arc<dyn DelegationSigner> {
    let signer: Arc<dyn DelegationSigner> =
        Arc::new(Ed25519Signer::from_slice(&spec.owner.key).expect("key"));
    assert_eq!(signer.did(), spec.owner.did.as_str());
    signer
}

/// The builder every peer starts from: its store, its key, iroh on the
/// peer's port (one the system chooses on its first start), and unsigned
/// writes when asked for.
fn builder(spec: &PeerSpec) -> defra_node::NodeBuilder {
    let builder = Node::builder()
        .data_path(spec.home.join("data"))
        .with_signing_key(SigningKey::Ed25519(spec.owner.key.clone()))
        .with_p2p(P2PConfig {
            peer_key: PeerKeySource::Store,
            bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            port: spec.port,
            relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
            discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
            ..Default::default()
        });
    match spec.blocks {
        Blocks::Signed => builder,
        Blocks::Unsigned => builder.with_identity_only(),
    }
}

/// A peer on a deployment of its own, serving its JSON-RPC methods on `rpc`
/// when given.
pub(crate) async fn own(
    spec: &PeerSpec,
    chain_id: u64,
    rpc: Option<SocketAddr>,
) -> defra_node::Result<Node> {
    builder(spec)
        .with_replica_registration(ReplicaRegistration::new(
            ["Users"],
            Arc::new(Deployment {
                own: vec![spec.owner.did.clone()],
            }),
        ))
        .with_embedded_vera(EmbeddedVeraConfig {
            data_dir: spec.home.join("vera"),
            deployment: EmbeddedVeraDeployment {
                chain_id,
                threshold: 1,
                operator_keys: vec![ACCOUNT_KEY.to_string()],
                created: now(),
                policies: vec![EmbeddedVeraPolicy {
                    definition: POLICY.to_string(),
                    counter: 0,
                    owner: spec.owner.did.to_string(),
                }],
            },
            listen: "127.0.0.1:0".parse().expect("address"),
            start_deadline: Duration::from_secs(60),
            stop_deadline: Duration::from_secs(30),
            actor: EmbeddedVeraActor::Signer(signer(spec)),
            rpc_listen: rpc,
            worker_dir: spec.home.join("worker"),
            keyring: keyring(spec),
            tuning: vera::AcpTuning::default(),
        })
        .build()
        .await
}

/// A peer that verifies against another peer's deployment.
pub(crate) async fn shared(spec: &PeerSpec, rpc: SocketAddr, trust: &EmbeddedVeraTrust) -> Node {
    builder(spec)
        .with_vera_rs(VeraRsConfig {
            rpc_address: format!("http://{rpc}"),
            consensus_key: trust.consensus_key.clone(),
            deployment_id: trust.chain_id,
            actor: VeraRsActor::Signer(signer(spec)),
            worker_dir: spec.home.join("worker"),
            keyring: keyring(spec),
            tuning: vera::AcpTuning::default(),
        })
        .build()
        .await
        .expect("node")
}

/// A peer on a deployment of its own that serves its JSON-RPC methods, and
/// the loopback address it serves them on.
pub(crate) async fn own_serving(spec: &PeerSpec, chain_id: u64) -> (Node, SocketAddr) {
    on_free_rpc_port(|rpc| own(spec, chain_id, Some(rpc))).await
}

/// What `start` builds, its Vera node serving JSON-RPC on a loopback port.
/// The port is found free and released before the Vera node binds it, so
/// another process can take it in between: a start that loses its port is
/// made again on a new one, and no other failure is.
async fn on_free_rpc_port<T, F, Fut>(mut start: F) -> (T, SocketAddr)
where
    F: FnMut(SocketAddr) -> Fut,
    Fut: std::future::Future<Output = defra_node::Result<T>>,
{
    let mut attempt = 1;
    loop {
        let rpc = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .and_then(|listener| listener.local_addr())
            .expect("free loopback port");
        match start(rpc).await {
            Ok(started) => return (started, rpc),
            Err(defra_node::Error::EmbeddedVera(host)) if attempt < 8 && host.is_rpc_bind() => {
                attempt += 1
            }
            Err(error) => panic!("node: {error:?}"),
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}
