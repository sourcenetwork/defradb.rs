//! Node 0's KMS releases a policy-bound document's key to the DID the iroh
//! handshake authenticated. Each test pairs node 0 with one peer, identical
//! but for a `reader` grant to the peer's node identity.
#![cfg(feature = "iroh")]

mod support;

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, ensure, Context, Result};
use blockstore::Blockstore;
use embedded::{EmbeddedNodeConfig, IrohConfig, SigningConfig, TransportConfig};
use identity::Did;
use tokio::time::{sleep, Instant};

use support::{
    add_policy, add_schema, grant_collection_reader, new_identity, users_as, wait_for_fred, Node,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_granted_iroh_peer_obtains_the_key() -> Result<()> {
    let pair = Pair::start(true).await?;
    let result = wait_for_fred(&pair.peer, &pair.jack).await;
    pair.shutdown().await;
    result
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_ungranted_iroh_peer_receives_the_blocks_but_not_the_key() -> Result<()> {
    let pair = Pair::start(false).await?;
    let result = assert_blocks_without_plaintext(&pair).await;
    pair.shutdown().await;
    result
}

struct Pair {
    node0: Node,
    peer: Node,
    jack: Did,
    doc_id: String,
}

impl Pair {
    async fn start(grant: bool) -> Result<Self> {
        let jack = new_identity()?;
        let peer_id = new_identity()?;
        let node0 = build_node(SigningConfig::Disabled).await?;
        let peer = build_node(SigningConfig::RegisteredIdentity {
            did: peer_id.to_string(),
        })
        .await?;

        match replicate_encrypted_doc(&node0, &peer, &jack, grant.then_some(&peer_id)).await {
            Ok(doc_id) => Ok(Self {
                node0,
                peer,
                jack,
                doc_id,
            }),
            Err(error) => {
                node0.shutdown().await;
                peer.shutdown().await;
                Err(error)
            }
        }
    }

    async fn shutdown(self) {
        self.node0.shutdown().await;
        self.peer.shutdown().await;
    }
}

async fn replicate_encrypted_doc(
    node0: &Node,
    peer: &Node,
    jack: &Did,
    grantee: Option<&Did>,
) -> Result<String> {
    let policy_id = add_policy(node0).await?;
    ensure!(
        policy_id == add_policy(peer).await?,
        "policy ids must match"
    );

    let sdl = format!(
        r#"type Users @branchable @policy(id: "{policy_id}", resource: "users") {{ name: String  age: Int }}"#
    );
    add_schema(node0, &sdl, jack).await?;
    add_schema(peer, &sdl, jack).await?;

    if let Some(grantee) = grantee {
        let collection_id = node0
            .database
            .get_collection("Users")?
            .context("Users collection missing on node 0")?
            .collection_id()
            .to_string();
        grant_collection_reader(node0, jack, grantee, &policy_id, &collection_id).await?;
        grant_collection_reader(peer, jack, grantee, &policy_id, &collection_id).await?;
    }

    let addr0 = connectable_iroh_addr(node0.p2p().context("node 0 has no p2p")?).await?;
    peer.p2p()
        .context("peer has no p2p")?
        .ops()
        .connect_peer(&addr0)
        .await
        .map_err(|e| anyhow!(e))?;
    for node in [node0, peer] {
        node.p2p()
            .context("node has no p2p")?
            .ops()
            .add_collections(vec!["Users".into()])
            .await
            .map_err(|e| anyhow!(e))?;
    }

    let response = node0
        .execute(
            r#"mutation { add_Users(input: {name: "Fred", age: 33}, encrypt: true) { _docID } }"#,
        )
        .await;
    if response.has_errors() {
        bail!("create on node 0 failed: {:?}", response.errors);
    }
    response
        .data
        .as_ref()
        .and_then(|data| data.pointer("/add_Users/0/_docID"))
        .and_then(|doc_id| doc_id.as_str())
        .map(str::to_string)
        .context("add_Users response missing _docID")
}

/// Without the head blocks on the peer, a missing plaintext would only show
/// that nothing replicated.
async fn assert_blocks_without_plaintext(pair: &Pair) -> Result<()> {
    let heads = db::merge::load_document_head_blocks(&pair.node0.database, &pair.doc_id)
        .await
        .map_err(|e| anyhow!(e))?;
    ensure!(!heads.is_empty(), "node 0 has no head for the document");

    let peer_blocks =
        blockstore::DefraBlockstore::new(Arc::clone(pair.peer.database.store()), true);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let mut received = true;
        for (cid, _) in &heads {
            received &= peer_blocks.has(cid).await.map_err(|e| anyhow!("{e}"))?;
        }
        if received {
            break;
        }
        if Instant::now() >= deadline {
            bail!("the ungranted peer never received the document's head blocks");
        }
        sleep(Duration::from_millis(250)).await;
    }

    // A granted peer decrypts within a couple of seconds of receiving them.
    let settled = Instant::now() + Duration::from_secs(5);
    while Instant::now() < settled {
        let response = users_as(&pair.peer, &pair.jack, "name").await;
        let decrypted = response
            .data
            .as_ref()
            .and_then(|data| data.get("Users"))
            .and_then(|users| users.as_array())
            .is_some_and(|users| {
                users
                    .iter()
                    .any(|user| user.get("name") == Some(&serde_json::json!("Fred")))
            });
        ensure!(!decrypted, "the ungranted peer decrypted the document");
        sleep(Duration::from_millis(250)).await;
    }
    Ok(())
}

async fn build_node(signing: SigningConfig) -> Result<Node> {
    let config = EmbeddedNodeConfig {
        transport: TransportConfig::Iroh(IrohConfig {
            bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            bind_port: Some(0),
            relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
            discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
            ..Default::default()
        }),
        signing,
        ..Default::default()
    };
    embedded::build_with_store(Arc::new(storage::RegolithStore::in_memory()?), config).await
}

async fn connectable_iroh_addr(system: &embedded::ManagedP2PSystem) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let addrs = system
            .ops()
            .listen_addresses()
            .await
            .map_err(|e| anyhow!(e))?;
        if let Some(addr) = addrs
            .into_iter()
            .find(|addr| addr.contains("/p2p/") || addr.starts_with("endpoint"))
        {
            return Ok(addr);
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for a connectable iroh address");
        }
        sleep(Duration::from_millis(100)).await;
    }
}
