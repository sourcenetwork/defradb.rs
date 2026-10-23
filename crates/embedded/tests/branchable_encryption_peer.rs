//! Guards #1603 on the embedded path: a `@branchable` collection under a DAC
//! policy, a `reader` grant on the collection object to node 1's node identity,
//! a pubsub subscription (no replicator), and an anonymous encrypted create on
//! node 0 that the owner then reads on node 1. Node 1 carries its node identity
//! from construction because node 0's KMS authenticates the key request against
//! it, as Go does (`cbindings/node_new.go` sets it independently of signing).
#![cfg(feature = "libp2p")]

#[path = "support/clock.rs"]
mod clock;
#[path = "support/outcome.rs"]
mod outcome;
mod support;

use std::sync::Arc;
use std::time::Duration;

use clock::sleep;
use embedded::{EmbeddedNodeConfig, Libp2pConfig, SigningConfig, TransportConfig};
use identity::Did;
use outcome::Result;
use std::time::Instant;

use support::{add_policy, add_schema, grant_collection_reader, new_identity, wait_for_fred, Node};

#[test]
fn branchable_grant_syncs_plain_doc_to_peer() -> Result<()> {
    edge::testing::edge()
        .block_on(async { run_scenario(false).await })
        .unwrap()
}

#[test]
fn branchable_grant_syncs_encrypted_doc_to_peer() -> Result<()> {
    edge::testing::edge()
        .block_on(async { run_scenario(true).await })
        .unwrap()
}

async fn run_scenario(encrypt: bool) -> Result<()> {
    let jack = new_identity()?;
    let node1_id = new_identity()?;

    let node0 = build_node(SigningConfig::Disabled).await?;
    let node1 = build_node(SigningConfig::RegisteredIdentity {
        did: node1_id.to_string(),
    })
    .await?;

    let result = scenario(&node0, &node1, &jack, &node1_id, encrypt).await;

    node0.shutdown().await;
    node1.shutdown().await;
    result
}

async fn scenario(
    node0: &Node,
    node1: &Node,
    jack: &Did,
    node1_id: &Did,
    encrypt: bool,
) -> Result<()> {
    let policy_id = add_policy(node0).await?;
    let policy_id1 = add_policy(node1).await?;
    assert_eq!(
        policy_id, policy_id1,
        "local policy ids must match across nodes"
    );

    let sdl = format!(
        r#"type Users @branchable @policy(id: "{policy_id}", resource: "users") {{ name: String  age: Int }}"#
    );
    add_schema(node0, &sdl, jack).await?;
    add_schema(node1, &sdl, jack).await?;

    let collection_id = node0
        .database
        .get_collection("Users")?
        .expect("Users collection missing on node 0")
        .collection_id()
        .to_string();
    grant_collection_reader(node0, jack, node1_id, &policy_id, &collection_id).await?;
    grant_collection_reader(node1, jack, node1_id, &policy_id, &collection_id).await?;

    let p2p0 = node0.p2p().expect("node 0 has no p2p");
    let p2p1 = node1.p2p().expect("node 1 has no p2p");
    let addr0 = wait_for_listen_addr(p2p0).await?;
    p2p1.ops().connect_peer(&addr0).await?;
    p2p0.ops().add_collections(vec!["Users".into()]).await?;
    p2p1.ops().add_collections(vec!["Users".into()]).await?;

    let encrypt_arg = if encrypt { ", encrypt: true" } else { "" };
    let mutation = format!(
        r#"mutation {{ add_Users(input: {{name: "Fred", age: 33}}{encrypt_arg}) {{ _docID }} }}"#
    );
    let response = node0.execute(&mutation).await;
    if response.has_errors() {
        panic!("create on node 0 failed: {:?}", response.errors);
    }

    wait_for_fred(node1, jack).await
}

async fn build_node(signing: SigningConfig) -> Result<Node> {
    let config = EmbeddedNodeConfig {
        edge: Some(edge::testing::edge()),
        transport: TransportConfig::Libp2p(Libp2pConfig {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
            ..Default::default()
        }),
        signing,
        ..Default::default()
    };
    embedded::build_with_store(Arc::new(storage::RegolithStore::in_memory()?), config)
        .await
        .map_err(Into::into)
}

async fn wait_for_listen_addr(system: &embedded::ManagedP2PSystem) -> Result<String> {
    let peer_id = system.ops().local_peer_id().await?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let addrs = system.ops().listen_addresses().await?;
        if let Some(addr) = addrs
            .into_iter()
            .find(|addr| addr.starts_with("/ip4/127.0.0.1/tcp/"))
        {
            return Ok(format!("{addr}/p2p/{peer_id}"));
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for a libp2p listen address");
        }
        sleep(Duration::from_millis(100)).await;
    }
}
