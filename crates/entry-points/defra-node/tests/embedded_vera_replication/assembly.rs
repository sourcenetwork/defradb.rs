//! How a peer's node is built: its store, keys, transport and Vera
//! deployment.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use defra_node::{
    EmbeddedVeraActor, EmbeddedVeraConfig, EmbeddedVeraDeployment, EmbeddedVeraPolicy,
    EmbeddedVeraTrust, Node, P2PConfig, PeerKeySource, ReplicaGrant, ReplicaOwnership,
    ReplicaRegistration, SigningKey, VeraActor, VeraConfig,
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

/// The host's opt-in: a `Notes` row's genesis grants `reader` to the node
/// its immutable `addressee` names. Nothing else is projected.
struct Addressed;

#[async_trait::async_trait]
impl db::DocumentMutationProjector for Addressed {
    fn genesis_read_access(
        &self,
        collection: &str,
        _policy: &str,
        _resource: &str,
    ) -> Option<db::GenesisReadAccess> {
        (collection == "Notes").then_some(db::GenesisReadAccess::AddressedBy("addressee"))
    }

    async fn reconcile(
        &self,
        _context: &db::DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        _context: &db::DocumentMutationContext<'_>,
        _collection: &str,
        _doc_id: &str,
    ) -> storage::corekv::Result<()> {
        Ok(())
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
        .with_document_projector(Arc::new(Addressed))
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
                policies: vec![seed()],
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

/// The DID every deployment's [`POLICY`] is seeded under. A collection
/// names its policy's ID, so deployments that replicate one collection to
/// each other hold the policy under one ID: one creator and one nonce.
const POLICY_CREATOR: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

/// [`POLICY`] as every deployment is seeded with it.
pub(crate) fn seed() -> EmbeddedVeraPolicy {
    EmbeddedVeraPolicy {
        definition: POLICY.to_string(),
        nonce: defra_node::PolicyNonce::default(),
        creator: POLICY_CREATOR.to_string(),
    }
}

/// A peer that verifies against another peer's deployment.
pub(crate) async fn shared(spec: &PeerSpec, rpc: SocketAddr, trust: &EmbeddedVeraTrust) -> Node {
    builder(spec)
        .with_vera(VeraConfig {
            rpc_address: format!("http://{rpc}"),
            consensus_key: trust.consensus_key.clone(),
            genesis_id: None,
            deployment_id: trust.chain_id,
            actor: VeraActor::Signer(signer(spec)),
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
    let node = own(
        spec,
        chain_id,
        Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
    )
    .await
    .expect("hosted node");
    let rpc = node.embedded_vera_rpc_addr().expect("bound RPC listener");
    (node, rpc)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}
