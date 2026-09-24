use integration_test::{extract_p2p_addr, TestCluster};
use serial_test::serial;

use super::blocks::{author, genesis, user, Signing};
use super::peer::HostilePeer;
use super::receiver::{describe, p2p_addrs, wait_for_user, wait_listening, SCHEMA};

fn commit_signers(cluster: &TestCluster, node: usize, doc_id: &str) -> Vec<String> {
    let commits = cluster
        .client(node)
        .query(&format!(
            r#"query {{ _commits(docID: "{doc_id}") {{ signature {{ identity }} }} }}"#
        ))
        .expect("commits query");
    commits["_commits"]
        .as_array()
        .expect("commits")
        .iter()
        .filter_map(|commit| commit["signature"]["identity"].as_str())
        .map(String::from)
        .collect()
}

/// An author's peer pushes to node0, which relays to node1. node0 signs its
/// own writes with a different key, so a relay that re-signed would show.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn the_authors_signature_survives_the_hop() {
    let writer = author(0x11);
    let relay = author(0x33);

    let cluster = TestCluster::builder()
        .rust_nodes(2)
        .with_iroh_transport()
        .with_signing()
        .with_node_identity(0, relay.private_key_hex.clone())
        .build()
        .await
        .expect("nodes start");
    wait_listening(&cluster, 2).await;
    let relay_node = cluster.client(0);
    let far_node = cluster.client(1);
    relay_node.schema_add(SCHEMA).expect("schema node0");
    far_node.schema_add(SCHEMA).expect("schema node1");
    let far_addr = extract_p2p_addr(&cluster, 1);
    relay_node.p2p_connect(&[&far_addr]).expect("connect");
    relay_node
        .p2p_collection_add(&["User"])
        .expect("node0 collection");
    far_node
        .p2p_collection_add(&["User"])
        .expect("node1 collection");
    relay_node
        .p2p_replicator_set(&["User"], &far_addr)
        .expect("replicator");
    let collection = describe(&relay_node, "User");

    let peer = HostilePeer::dial(&p2p_addrs(&cluster, 0)).await;
    let composite_only = genesis(
        &user("Composite", 1),
        &collection.version_id,
        &writer,
        Signing::CompositeOnly,
    );
    let every_block = genesis(
        &user("Every", 2),
        &collection.version_id,
        &writer,
        Signing::EveryBlock,
    );
    for fragment in [&composite_only, &every_block] {
        peer.push(fragment, &collection.collection_id).await;
    }

    for fragment in [&composite_only, &every_block] {
        wait_for_user(&far_node, None, &fragment.doc_id, |_| true).await;
        let signers = commit_signers(&cluster, 1, &fragment.doc_id);
        assert!(
            signers.contains(&writer.public_key_hex),
            "node1 must hold the author's signature, got {signers:?}"
        );
        assert!(
            !signers.contains(&relay.public_key_hex),
            "the relaying node's key must not replace the author's, got {signers:?}"
        );
    }
}
