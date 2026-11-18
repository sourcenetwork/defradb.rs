//! DefraDB nodes that each run a Vera deployment of their own, or share
//! another node's, and replicate to each other over iroh on localhost.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use defra_node::{EmbeddedVeraTrust, Node};

use super::actor::Actor;
use super::assembly::{free_udp_port, own, own_serving, shared, POLICY};

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
    /// The UDP port the peer listens on, the same across restarts, so a
    /// replicator that names it reaches the restarted peer.
    pub port: u16,
    pub provider: Provider,
    pub owner: Actor,
    pub blocks: Blocks,
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
            port: free_udp_port(),
            provider: Provider::Own {
                chain_id,
                serves: false,
            },
            owner: owner.clone(),
            blocks,
        })
        .await
    }

    /// A peer on a deployment of its own that other peers share.
    pub async fn start_serving(home: &Path, chain_id: u64, owner: &Actor) -> Self {
        Self::from_spec(PeerSpec {
            home: home.to_path_buf(),
            port: free_udp_port(),
            provider: Provider::Own {
                chain_id,
                serves: true,
            },
            owner: owner.clone(),
            blocks: Blocks::Signed,
        })
        .await
    }

    /// A peer on `host`'s deployment.
    pub async fn start_sharing(home: &Path, host: &Peer, owner: &Actor) -> Self {
        Self::from_spec(PeerSpec {
            home: home.to_path_buf(),
            port: free_udp_port(),
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
        let owner = &spec.owner;
        let policy = node
            .add_dac_policy_with_counter_as(
                &defra_core::RequestContext::default().with_identity(Some(owner.did.to_string())),
                POLICY,
                0,
            )
            .await
            .expect("seeded policy");
        if node.get_collection("Users").expect("collections").is_none() {
            node.add_collection(&format!(
                "type Users @policy(id: \"{policy}\", resource: \"users\") {{ \
                 name: String points: Int @crdt(type: pcounter) }}"
            ))
            .await
            .expect("collection");
        }
        Self {
            node,
            spec,
            policy,
            rpc,
        }
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
