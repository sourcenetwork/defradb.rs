//! Vera-backed document ACP over iroh replication: subscription, replicators
//! and document-actor relationships.
//!
//! Ported from Go: tests/integration/acp/dac/p2p/

use std::time::Duration;

use integration_test::{
    extract_p2p_addr, generate_identity, poll_until, users_schema_with_policy, TestCluster,
    USER_ACP_POLICY,
};
use serial_test::serial;

use super::helpers;

mod strict_updates;

const P2P_TIMEOUT: Duration = Duration::from_secs(15);

fn extract_policy_id(output: &serde_json::Value) -> String {
    output["PolicyID"]
        .as_str()
        .or_else(|| output["policyID"].as_str())
        .or_else(|| output.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| output.to_string().trim_matches('"').to_string())
}

/// Helper: extract doc_id from create mutation response.
fn extract_doc_id(data: &serde_json::Value, mutation_name: &str) -> String {
    data[mutation_name]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v["_docID"].as_str())
        .or_else(|| data[mutation_name]["_docID"].as_str())
        .expect("missing _docID")
        .to_string()
}

/// A vera.rs deployment and two iroh nodes that take document ACP from it and
/// sign their blocks, with the policy and schema on both.
struct VeraCluster {
    cluster: TestCluster,
    owner_key: String,
    _hub: vera_harness::cluster::TestCluster,
}

async fn setup_vera_cluster() -> VeraCluster {
    setup_vera_cluster_with_relay(None).await
}

async fn setup_vera_cluster_with_relay(relay_key: Option<&str>) -> VeraCluster {
    let owner_key = helpers::funded_identity().private_key_hex;
    let hub = helpers::start_hub_cluster().await;
    let mut builder = helpers::vera_builder(&hub.node(0).rpc_url(), &owner_key, 2, false)
        .with_iroh_transport()
        // vera.rs decides an inbound update or delete by the block's signer.
        .with_signing()
        .with_rust_binary(integration_test::BinarySource::Path(
            helpers::defra_iroh_binary(),
        ));
    if let Some(relay_key) = relay_key {
        builder = builder.with_node_identity(1, relay_key);
    }
    let cluster = builder.build().await.expect("build Vera cluster");

    for i in 0..2 {
        cluster
            .wait_for_log(i, "p2p_listening", P2P_TIMEOUT)
            .await
            .unwrap_or_else(|_| panic!("node{} P2P listener", i));
    }

    let node0 = cluster.client(0);
    let node1 = cluster.client(1);
    let policy_result = node0
        .acp_policy_add(USER_ACP_POLICY, &owner_key)
        .expect("add policy on node0");
    let schema = users_schema_with_policy(&extract_policy_id(&policy_result));
    node0
        .schema_add_with_identity(&schema, &owner_key)
        .expect("schema node0");
    node1
        .schema_add_with_identity(&schema, &owner_key)
        .expect("schema node1");

    VeraCluster {
        cluster,
        owner_key,
        _hub: hub,
    }
}

/// Helper: connect two nodes and set up replicator (node0 → node1).
fn setup_replicator(cluster: &TestCluster) {
    let node0 = cluster.client(0);
    let node1 = cluster.client(1);
    let addr1 = extract_p2p_addr(cluster, 1);

    node0.p2p_connect(&[&addr1]).expect("connect");
    node0.p2p_collection_add(&["User"]).expect("col node0");
    node1.p2p_collection_add(&["User"]).expect("col node1");
    node0
        .p2p_replicator_set(&["User"], &addr1)
        .expect("replicator 0→1");
}

/// Port: TestACP_P2PSubscribeAddGetSingleWithPermissionedCollection_VeraACP
/// Subscription-based sync with Vera ACP enforcement.
#[nuclei::test]
#[serial]
async fn subscribe_add_get_permissioned_vera() {
    edge::compat(async move {
        let VeraCluster {
            cluster,
            owner_key,
            _hub,
        } = setup_vera_cluster().await;

        setup_replicator(&cluster);

        let node0 = cluster.client(0);
        let node1 = cluster.client(1);

        // Create doc as owner on node0
        node0
            .query_with_identity(
                r#"mutation { add_User(input: {name: "Fred", age: 30}) { _docID } }"#,
                &owner_key,
            )
            .expect("create Fred on node0");

        // Owner can read on node1 after replication
        let node1_ref = &node1;
        let key = owner_key.clone();
        poll_until(
            || {
                let r = node1_ref
                    .query_with_identity("query { User { name } }", &key)
                    .unwrap_or_default();
                r["User"]
                    .as_array()
                    .map(|arr| arr.iter().any(|u| u["name"].as_str() == Some("Fred")))
                    .unwrap_or(false)
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "Vera ACP-protected doc did not replicate",
        )
        .await;

        // Anonymous cannot read on node1
        let anon = node1.query("query { User { name } }").unwrap_or_default();
        let anon_count = anon["User"].as_array().map(|arr| arr.len()).unwrap_or(0);
        assert_eq!(
            anon_count, 0,
            "anonymous should NOT see docs on node1 (Vera ACP enforced)"
        );
    })
    .await;
}

/// Port: TestACP_P2PCreatePrivateDocumentsOnDifferentNodes_VeraACP
/// Create private docs on different nodes, verify isolation.
#[nuclei::test]
#[serial]
async fn create_private_docs_different_nodes() {
    edge::compat(async move {
        let VeraCluster {
            cluster,
            owner_key,
            _hub,
        } = setup_vera_cluster().await;

        let node0 = cluster.client(0);
        let node1 = cluster.client(1);

        // Create separate docs on each node
        node0
            .query_with_identity(
                r#"mutation { add_User(input: {name: "Alice", age: 25}) { _docID } }"#,
                &owner_key,
            )
            .expect("create Alice on node0");

        node1
            .query_with_identity(
                r#"mutation { add_User(input: {name: "Bob", age: 30}) { _docID } }"#,
                &owner_key,
            )
            .expect("create Bob on node1");

        // Owner sees their docs on each node (no replicator → docs stay local)
        let node0_docs = node0
            .query_with_identity("query { User { name } }", &owner_key)
            .expect("query node0");
        let node1_docs = node1
            .query_with_identity("query { User { name } }", &owner_key)
            .expect("query node1");

        let n0_names: Vec<&str> = node0_docs["User"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|u| u["name"].as_str())
            .collect();
        let n1_names: Vec<&str> = node1_docs["User"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|u| u["name"].as_str())
            .collect();

        assert!(n0_names.contains(&"Alice"), "node0 should have Alice");
        assert!(n1_names.contains(&"Bob"), "node1 should have Bob");

        // Anonymous sees nothing
        let anon0 = node0.query("query { User { name } }").unwrap_or_default();
        assert_eq!(
            anon0["User"].as_array().map(|a| a.len()).unwrap_or(0),
            0,
            "anonymous should see nothing on node0"
        );
    })
    .await;
}

/// Port: TestACP_P2PCreatePrivateDocumentAndSyncAfterAddingRelationship_VeraACP
/// Private doc becomes visible after granting reader relationship.
#[nuclei::test]
#[serial]
async fn create_private_sync_after_relationship() {
    edge::compat(async move {
        let VeraCluster {
            cluster,
            owner_key,
            _hub,
        } = setup_vera_cluster().await;

        setup_replicator(&cluster);

        let node0 = cluster.client(0);
        let node1 = cluster.client(1);

        // Generate a second identity (reader)
        let binary = integration_test::rust_binary();
        let reader = generate_identity(&binary).expect("generate reader identity");

        // Create private doc as owner
        let data = node0
            .query_with_identity(
                r#"mutation { add_User(input: {name: "Secret", age: 99}) { _docID } }"#,
                &owner_key,
            )
            .expect("create Secret");
        let doc_id = extract_doc_id(&data, "add_User");

        // Wait for replication
        nuclei::time::sleep(Duration::from_secs(3)).await;

        // Reader cannot see the doc on node1
        let reader_before = node1
            .query_with_identity("query { User { name } }", &reader.private_key_hex)
            .unwrap_or_default();
        let before_count = reader_before["User"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(
            before_count, 0,
            "reader should NOT see private doc before relationship"
        );

        // Grant reader relationship
        node0
            .acp_relationship_add("User", &doc_id, "reader", &reader.did, &owner_key)
            .expect("add reader relationship");

        // Reader can now see the doc
        let reader_ref = &node1;
        let key = reader.private_key_hex.clone();
        poll_until(
            || {
                reader_ref
                    .query_with_identity("query { User { name } }", &key)
                    .unwrap_or_default()["User"]
                    .as_array()
                    .is_some_and(|users| users.len() == 1)
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "reader never saw the doc after the relationship grant",
        )
        .await;
        let reader_after = node1
            .query_with_identity("query { User { name } }", &reader.private_key_hex)
            .expect("reader query after relationship");
        let after_count = reader_after["User"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(
            after_count, 1,
            "reader should see doc after relationship grant"
        );
    })
    .await;
}

/// Port: TestACP_P2PUpdatePrivateDocumentsOnDifferentNodes_VeraACP
/// Update private docs on different nodes, verify sync.
#[nuclei::test]
#[serial]
async fn update_private_docs_different_nodes() {
    edge::compat(async move {
        let VeraCluster {
            cluster,
            owner_key,
            _hub,
        } = setup_vera_cluster().await;

        setup_replicator(&cluster);

        let node0 = cluster.client(0);
        let node1 = cluster.client(1);

        // Create doc on node0
        let data = node0
            .query_with_identity(
                r#"mutation { add_User(input: {name: "Eve", age: 20}) { _docID } }"#,
                &owner_key,
            )
            .expect("create Eve");
        let doc_id = extract_doc_id(&data, "add_User");

        // Wait for replication
        let node1_ref = &node1;
        let key = owner_key.clone();
        poll_until(
            || {
                let r = node1_ref
                    .query_with_identity("query { User { name } }", &key)
                    .unwrap_or_default();
                r["User"]
                    .as_array()
                    .map(|arr| arr.iter().any(|u| u["name"].as_str() == Some("Eve")))
                    .unwrap_or(false)
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "Eve did not replicate to node1",
        )
        .await;

        // Update on node0
        node0
            .query_with_identity(
                &format!(
                    r#"mutation {{ update_User(docID: "{}", input: {{age: 21}}) {{ _docID }} }}"#,
                    doc_id
                ),
                &owner_key,
            )
            .expect("update Eve age");

        // Wait for update to replicate
        poll_until(
            || {
                let r = node1_ref
                    .query_with_identity("query { User { name age } }", &key)
                    .unwrap_or_default();
                r["User"]
                    .as_array()
                    .map(|arr| arr.iter().any(|u| u["age"].as_i64() == Some(21)))
                    .unwrap_or(false)
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "update did not replicate to node1",
        )
        .await;

        // Anonymous still cannot see
        let anon = node1.query("query { User { name } }").unwrap_or_default();
        assert_eq!(
            anon["User"].as_array().map(|a| a.len()).unwrap_or(0),
            0,
            "anonymous should NOT see updated doc"
        );
    })
    .await;
}

/// Port: TestACP_P2PDeletePrivateDocumentsOnDifferentNodes_VeraACP
/// Delete private docs, verify deletion syncs.
#[nuclei::test]
#[serial]
async fn delete_private_docs_different_nodes() {
    edge::compat(async move {
        let VeraCluster {
            cluster,
            owner_key,
            _hub,
        } = setup_vera_cluster().await;

        setup_replicator(&cluster);

        let node0 = cluster.client(0);
        let node1 = cluster.client(1);

        // Create and replicate doc
        let data = node0
            .query_with_identity(
                r#"mutation { add_User(input: {name: "ToDelete", age: 50}) { _docID } }"#,
                &owner_key,
            )
            .expect("create ToDelete");
        let doc_id = extract_doc_id(&data, "add_User");

        let node1_ref = &node1;
        let key = owner_key.clone();
        poll_until(
            || {
                let r = node1_ref
                    .query_with_identity("query { User { name } }", &key)
                    .unwrap_or_default();
                r["User"]
                    .as_array()
                    .map(|arr| arr.iter().any(|u| u["name"].as_str() == Some("ToDelete")))
                    .unwrap_or(false)
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "ToDelete did not replicate",
        )
        .await;

        // Delete on node0
        node0
            .query_with_identity(
                &format!(
                    r#"mutation {{ delete_User(docID: "{}") {{ _docID }} }}"#,
                    doc_id
                ),
                &owner_key,
            )
            .expect("delete doc");

        // Wait for deletion to replicate
        poll_until(
            || {
                let r = node1_ref
                    .query_with_identity("query { User { name _deleted } }", &key)
                    .unwrap_or_default();
                let users = r["User"].as_array();
                match users {
                    Some(arr) => {
                        // Either: doc has _deleted=true, or doc is gone entirely
                        arr.iter().all(|u| u["name"].as_str() != Some("ToDelete"))
                            || arr.iter().any(|u| {
                                u["name"].as_str() == Some("ToDelete")
                                    && u["_deleted"].as_bool() == Some(true)
                            })
                    }
                    None => false,
                }
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "deletion did not replicate",
        )
        .await;
    })
    .await;
}

/// Port: TestACP_P2POneToOneReplicatorWithPermissionedCollection_VeraACP
/// Replicator with Vera ACP enforcement.
#[nuclei::test]
#[serial]
async fn replicator_permissioned_vera() {
    edge::compat(async move {
        let VeraCluster {
            cluster,
            owner_key,
            _hub,
        } = setup_vera_cluster().await;

        setup_replicator(&cluster);

        let node0 = cluster.client(0);
        let node1 = cluster.client(1);

        // Create doc as owner
        node0
            .query_with_identity(
                r#"mutation { add_User(input: {name: "John", age: 21}) { _docID } }"#,
                &owner_key,
            )
            .expect("create John");

        // Owner can read on node1 after replication
        let node1_ref = &node1;
        let key = owner_key.clone();
        poll_until(
            || {
                let r = node1_ref
                    .query_with_identity("query { User { name } }", &key)
                    .unwrap_or_default();
                r["User"]
                    .as_array()
                    .map(|arr| arr.iter().any(|u| u["name"].as_str() == Some("John")))
                    .unwrap_or(false)
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "Vera ACP doc did not replicate via replicator",
        )
        .await;

        // Anonymous cannot read
        let anon = node1.query("query { User { name } }").unwrap_or_default();
        assert_eq!(
            anon["User"].as_array().map(|a| a.len()).unwrap_or(0),
            0,
            "anonymous should NOT see docs (Vera ACP enforced)"
        );

        // A different identity cannot read
        let binary = integration_test::rust_binary();
        let stranger = generate_identity(&binary).expect("stranger identity");
        let stranger_result = node1
            .query_with_identity("query { User { name } }", &stranger.private_key_hex)
            .unwrap_or_default();
        assert_eq!(
            stranger_result["User"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0),
            0,
            "unauthorized identity should NOT see docs"
        );
    })
    .await;
}

/// Port: TestACP_P2PSubscribeAddGetSingleWithPermissionedCollectionCreateDocActorRelationship_VeraACP
/// Subscription with doc-actor relationship grants access.
#[nuclei::test]
#[serial]
async fn subscribe_add_get_with_doc_actor_relationship() {
    edge::compat(async move {
        let VeraCluster {
            cluster,
            owner_key,
            _hub,
        } = setup_vera_cluster().await;

        setup_replicator(&cluster);

        let node0 = cluster.client(0);
        let node1 = cluster.client(1);

        let binary = integration_test::rust_binary();
        let reader = generate_identity(&binary).expect("reader identity");

        // Create doc as owner
        let data = node0
            .query_with_identity(
                r#"mutation { add_User(input: {name: "Carol", age: 28}) { _docID } }"#,
                &owner_key,
            )
            .expect("create Carol");
        let doc_id = extract_doc_id(&data, "add_User");

        // Wait for replication
        let node1_ref = &node1;
        let key = owner_key.clone();
        poll_until(
            || {
                let r = node1_ref
                    .query_with_identity("query { User { name } }", &key)
                    .unwrap_or_default();
                r["User"]
                    .as_array()
                    .map(|arr| arr.iter().any(|u| u["name"].as_str() == Some("Carol")))
                    .unwrap_or(false)
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "Carol did not replicate",
        )
        .await;

        // Reader cannot see yet
        let before = node1
            .query_with_identity("query { User { name } }", &reader.private_key_hex)
            .unwrap_or_default();
        assert_eq!(
            before["User"].as_array().map(|a| a.len()).unwrap_or(0),
            0,
            "reader should NOT see doc before relationship"
        );

        // Add reader relationship
        node0
            .acp_relationship_add("User", &doc_id, "reader", &reader.did, &owner_key)
            .expect("add reader relationship");
        let reader_ref = &node1;
        let key = reader.private_key_hex.clone();
        poll_until(
            || {
                reader_ref
                    .query_with_identity("query { User { name } }", &key)
                    .unwrap_or_default()["User"]
                    .as_array()
                    .is_some_and(|users| users.len() == 1)
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "reader never saw the doc after the relationship grant",
        )
        .await;

        // Reader can now see
        let after = node1
            .query_with_identity("query { User { name } }", &reader.private_key_hex)
            .expect("reader query after relationship");
        assert_eq!(
            after["User"].as_array().map(|a| a.len()).unwrap_or(0),
            1,
            "reader should see doc after relationship grant"
        );
    })
    .await;
}

/// Port: TestACP_P2PReplicatorWithPermissionedCollectionCreateDocActorRelationship_VeraACP
/// Replicator with doc-actor relationship management.
#[nuclei::test]
#[serial]
async fn replicator_with_doc_actor_relationship() {
    edge::compat(async move {
        let VeraCluster {
            cluster,
            owner_key,
            _hub,
        } = setup_vera_cluster().await;

        setup_replicator(&cluster);

        let node0 = cluster.client(0);
        let node1 = cluster.client(1);

        let binary = integration_test::rust_binary();
        let reader = generate_identity(&binary).expect("reader identity");

        // Create doc
        let data = node0
            .query_with_identity(
                r#"mutation { add_User(input: {name: "Dan", age: 35}) { _docID } }"#,
                &owner_key,
            )
            .expect("create Dan");
        let doc_id = extract_doc_id(&data, "add_User");

        // Wait for replication
        let node1_ref = &node1;
        let key = owner_key.clone();
        poll_until(
            || {
                let r = node1_ref
                    .query_with_identity("query { User { name } }", &key)
                    .unwrap_or_default();
                r["User"]
                    .as_array()
                    .map(|arr| arr.iter().any(|u| u["name"].as_str() == Some("Dan")))
                    .unwrap_or(false)
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "Dan did not replicate",
        )
        .await;

        // Reader cannot see
        let before = node1
            .query_with_identity("query { User { name } }", &reader.private_key_hex)
            .unwrap_or_default();
        assert_eq!(
            before["User"].as_array().map(|a| a.len()).unwrap_or(0),
            0,
            "reader should NOT see before relationship"
        );

        // Grant reader access on node0
        node0
            .acp_relationship_add("User", &doc_id, "reader", &reader.did, &owner_key)
            .expect("add reader relationship");

        // Verify idempotent: add same relationship from node1
        node1
            .acp_relationship_add("User", &doc_id, "reader", &reader.did, &owner_key)
            .expect("adding the same reader relationship again from node1 succeeds");
        let (on0, on1) = (&node0, &node1);
        let key = reader.private_key_hex.clone();
        poll_until(
            || {
                [on0, on1].iter().all(|node| {
                    node.query_with_identity("query { User { name } }", &key)
                        .unwrap_or_default()["User"]
                        .as_array()
                        .is_some_and(|users| users.len() == 1)
                })
            },
            Duration::from_secs(15),
            Duration::from_millis(300),
            "reader never saw the doc on both nodes after the relationship grant",
        )
        .await;

        // Reader can now see on both nodes
        let on_node0 = node0
            .query_with_identity("query { User { name } }", &reader.private_key_hex)
            .expect("reader on node0");
        let on_node1 = node1
            .query_with_identity("query { User { name } }", &reader.private_key_hex)
            .expect("reader on node1");

        assert_eq!(
            on_node0["User"].as_array().map(|a| a.len()).unwrap_or(0),
            1,
            "reader should see doc on node0 after relationship"
        );
        assert_eq!(
            on_node1["User"].as_array().map(|a| a.len()).unwrap_or(0),
            1,
            "reader should see doc on node1 after relationship"
        );
    })
    .await;
}
