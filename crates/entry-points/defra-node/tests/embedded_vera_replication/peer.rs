//! DefraDB nodes that each run a Vera deployment of their own, or share
//! another node's, and replicate to each other over iroh on localhost.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use defra_node::{EmbeddedVeraTrust, ForeignHome, Node};

use super::actor::Actor;
use super::assembly::{own, own_serving, seed_of, shared};
use super::documents::transient;
use super::wait::eventually;

/// How a peer writes its blocks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blocks {
    /// Signed with the node's key, or the key a request carries.
    Signed,
    /// Unsigned unless a request carries a key.
    Unsigned,
}

/// Where a peer's Vera deployment runs.
#[derive(Clone)]
pub enum Provider {
    /// A single-validator deployment of the peer's own, `chain_id`, its
    /// JSON-RPC methods also served on loopback when `serves`.
    Own { chain_id: u64, serves: bool },
    /// The deployment another peer runs, reached over its JSON-RPC methods
    /// at `rpc`.
    Shared {
        rpc: SocketAddr,
        trust: EmbeddedVeraTrust,
    },
}

/// What a peer is started with, kept so it can be started again.
#[derive(Clone)]
pub struct PeerSpec {
    pub home: PathBuf,
    /// The UDP port the peer listens on: chosen by the system when it
    /// first binds, then kept across restarts, so a replicator that names
    /// it reaches the restarted peer.
    pub port: Option<u16>,
    pub provider: Provider,
    pub owner: Actor,
    pub blocks: Blocks,
    /// Who its deployment's `Users` policy is seeded by: the shared creator
    /// of every deployment when `None`, so the policy has one ID on all of
    /// them.
    pub seed_creator: Option<String>,
    /// The policy of another deployment its `Users` collection is under,
    /// decided against that deployment's home: routed before the
    /// collection is made, and again on every start.
    pub foreign: Option<ForeignRoute>,
    /// How often its governance sweep runs; `None` keeps the node's
    /// default.
    pub governance_sweep: Option<Duration>,
}

/// A policy another deployment owns, as a peer routes it: `spelled` as it
/// is added, which need not be how the collection names it.
#[derive(Clone)]
pub struct ForeignRoute {
    pub spelled: String,
    pub home: ForeignHome,
}

/// A DefraDB node whose Vera actor and node identity are `owner`. On a
/// deployment of its own it registers the replicas of `Users` other
/// deployments' actors create.
pub struct Peer {
    pub node: Node,
    pub spec: PeerSpec,
    pub policy: String,
    /// Where its own deployment serves JSON-RPC, when it serves.
    pub rpc: Option<SocketAddr>,
}

impl Peer {
    /// A peer on a deployment of its own.
    pub async fn start(home: &Path, chain_id: u64, owner: &Actor, blocks: Blocks) -> Self {
        Self::from_spec(PeerSpec {
            home: home.to_path_buf(),
            port: None,
            provider: Provider::Own {
                chain_id,
                serves: false,
            },
            owner: owner.clone(),
            blocks,
            seed_creator: None,
            foreign: None,
            governance_sweep: None,
        })
        .await
    }

    /// A peer on a deployment of its own that other peers share.
    pub async fn start_serving(home: &Path, chain_id: u64, owner: &Actor) -> Self {
        Self::from_spec(PeerSpec {
            home: home.to_path_buf(),
            port: None,
            provider: Provider::Own {
                chain_id,
                serves: true,
            },
            owner: owner.clone(),
            blocks: Blocks::Signed,
            seed_creator: None,
            foreign: None,
            governance_sweep: None,
        })
        .await
    }

    /// A peer on `host`'s deployment.
    pub async fn start_sharing(home: &Path, host: &Peer, owner: &Actor) -> Self {
        Self::from_spec(PeerSpec {
            home: home.to_path_buf(),
            port: None,
            provider: Provider::Shared {
                rpc: host.rpc.expect("the host serves its deployment"),
                trust: host
                    .node
                    .embedded_vera_trust()
                    .expect("the host runs its deployment")
                    .clone(),
            },
            owner: owner.clone(),
            blocks: Blocks::Signed,
            seed_creator: None,
            foreign: None,
            governance_sweep: None,
        })
        .await
    }

    pub async fn from_spec(spec: PeerSpec) -> Self {
        let (node, rpc) = match &spec.provider {
            Provider::Own {
                chain_id,
                serves: true,
            } => {
                let (node, rpc) = own_serving(&spec, *chain_id).await;
                (node, Some(rpc))
            }
            Provider::Own { chain_id, .. } => {
                (own(&spec, *chain_id, None).await.expect("node"), None)
            }
            Provider::Shared { rpc, trust } => (shared(&spec, *rpc, trust).await, None),
        };
        let policy = match &spec.foreign {
            Some(route) => {
                route_foreign(&node, route).await;
                route.spelled.trim_start_matches("0x").to_lowercase()
            }
            None => {
                let policy = seed_of(&spec).id().expect("policy ID");
                node.anonymous()
                    .get_dac_policy(&policy)
                    .await
                    .expect("policy read")
                    .expect("seeded policy");
                policy
            }
        };
        if node.get_collection("Users").expect("collections").is_none() {
            add_users(&node, &policy).await;
        }
        let mut peer = Self {
            node,
            spec,
            policy,
            rpc,
        };
        let port = peer.bound_port().await;
        assert!(
            peer.spec.port.is_none_or(|kept| kept == port),
            "a restarted peer listens on the port it had"
        );
        peer.spec.port = Some(port);
        peer
    }

    pub fn owner(&self) -> &Actor {
        &self.spec.owner
    }

    /// Stop this peer and start it again over the same directories.
    pub async fn restart(self) -> Self {
        let spec = self.spec.clone();
        self.node.shutdown().await.expect("shutdown");
        drop(self.node);
        Self::from_spec(spec).await
    }

    pub async fn stop(self) {
        self.node.shutdown().await.expect("shutdown");
    }
}

/// Route `route`'s policy to its home, asked again while the node's own
/// deployment, which must be shown not to hold it, is unavailable.
async fn route_foreign(node: &Node, route: &ForeignRoute) {
    eventually("the foreign policy is routed", || async {
        match node.add_foreign_policy(&route.spelled, &route.home).await {
            Ok(()) => true,
            Err(error) if unavailable(&error.to_string()) => false,
            Err(error) => panic!("foreign policy: {error}"),
        }
    })
    .await;
}

/// Make the `Users` collection under `policy`, asked again while the
/// deployment that decides the policy is unavailable.
async fn add_users(node: &Node, policy: &str) {
    let sdl = format!(
        "type Users @policy(id: \"{policy}\", resource: \"users\") {{ \
         name: String points: Int @crdt(type: pcounter) }}"
    );
    eventually("the Users collection is made", || async {
        match node.add_collection(&sdl).await {
            Ok(_) => true,
            Err(error) if unavailable(&error.to_string()) => false,
            Err(error) => panic!("collection: {error}"),
        }
    })
    .await;
}

/// Whether `message` says the deployment it asked could not answer yet.
fn unavailable(message: &str) -> bool {
    transient(message) || message.contains("Vera unavailable")
}
