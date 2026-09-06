use std::time::Duration;

use integration_test::{generate_identity, users_schema_with_policy, USER_ACP_POLICY};

use super::helpers;

/// Replicated documents and commit history follow Vera grants and revocation.
#[tokio::test]
#[serial_test::serial]
async fn rust_hubrs_p2p_acp() {
    let jack = helpers::funded_identity();
    let reader = generate_identity(&helpers::defra_binary()).expect("reader identity");

    let hub = helpers::start_hub_cluster().await;
    let hub_rpc_url = hub.node(0).rpc_url();

    let cluster =
        helpers::build_defra_with_hub_rs(&hub_rpc_url, &jack.private_key_hex, 2, true).await;
    let node0 = cluster.client(0);
    let node1 = cluster.client(1);

    // Add policy on hub.rs via node 0
    let policy_result = node0
        .acp_policy_add(USER_ACP_POLICY, &jack.private_key_hex)
        .expect("add policy");
    let policy_id = policy_result["PolicyID"]
        .as_str()
        .or_else(|| policy_result["policyID"].as_str())
        .expect("PolicyID");

    // Deploy schema on both nodes
    let schema = users_schema_with_policy(policy_id);
    node0
        .schema_add_with_identity(&schema, &jack.private_key_hex)
        .expect("schema on node0");

    node1
        .schema_add_with_identity(&schema, &jack.private_key_hex)
        .expect("schema on node1");

    // Get node1 multiaddr and connect
    let info1 = node1.p2p_info().expect("p2p info node1");
    let addr1 = info1
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .expect("node1 has no P2P address");

    node0.p2p_connect(&[addr1]).expect("p2p connect");
    node0
        .p2p_collection_add(&["User"])
        .expect("p2p collection add node0");
    node1
        .p2p_collection_add(&["User"])
        .expect("p2p collection add node1");
    node0
        .p2p_replicator_set_with_identity(&["User"], addr1, &jack.private_key_hex)
        .expect("set replicator");

    // Create document as Jack on node 0
    let created = node0
        .query_with_identity(
            r#"mutation { add_User(input: {name: "Jack", age: 30}) { _docID } }"#,
            &jack.private_key_hex,
        )
        .expect("create user on node0");
    let doc_id = created["add_User"][0]["_docID"]
        .as_str()
        .expect("document ID");

    // Poll until replication completes (up to 30s)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let jack_on_node1 = node1
            .query_with_identity("query { User { _docID name } }", &jack.private_key_hex)
            .expect("Jack query on node1");
        let users = jack_on_node1["User"].as_array().expect("users array");
        if users.len() == 1 {
            assert_eq!(users[0]["name"], "Jack");
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for replication to node 1"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let document_query = "query { User { _docID name } }";
    let history_query = format!(r#"query {{ _commits(docID: "{doc_id}") {{ cid }} }}"#);
    for node in [&node0, &node1] {
        for (query, field) in [
            (document_query, "User"),
            (history_query.as_str(), "_commits"),
        ] {
            let owner_result = node
                .query_with_identity(query, &jack.private_key_hex)
                .expect("owner read");
            assert!(!owner_result[field].as_array().unwrap().is_empty());
            let anonymous_result = node.query(query).expect("anonymous read");
            assert!(anonymous_result[field].as_array().unwrap().is_empty());
            let reader_result = node
                .query_with_identity(query, &reader.private_key_hex)
                .expect("read before grant");
            assert!(reader_result[field].as_array().unwrap().is_empty());
        }
    }

    node0
        .acp_relationship_add("User", doc_id, "reader", &reader.did, &jack.private_key_hex)
        .expect("grant reader");
    for node in [&node0, &node1] {
        for query in [document_query, history_query.as_str()] {
            let owner_result = node
                .query_with_identity(query, &jack.private_key_hex)
                .expect("owner read after grant");
            let reader_result = node
                .query_with_identity(query, &reader.private_key_hex)
                .expect("reader read after grant");
            assert_eq!(reader_result, owner_result);
        }
    }

    node0
        .acp_relationship_delete("User", doc_id, "reader", &reader.did, &jack.private_key_hex)
        .expect("revoke reader");
    for node in [&node0, &node1] {
        for (query, field) in [
            (document_query, "User"),
            (history_query.as_str(), "_commits"),
        ] {
            let reader_result = node
                .query_with_identity(query, &reader.private_key_hex)
                .expect("read after revocation");
            assert!(reader_result[field].as_array().unwrap().is_empty());
            let owner_result = node
                .query_with_identity(query, &jack.private_key_hex)
                .expect("owner read after revocation");
            assert!(!owner_result[field].as_array().unwrap().is_empty());
        }
    }
}
