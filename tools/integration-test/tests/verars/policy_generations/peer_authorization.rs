use std::time::Duration;

use integration_test::{
    generate_identity, users_schema_with_policy, DefraClient, TestCluster, USER_ACP_POLICY,
};
use serde_json::Value;

use super::{assert_visibility, commit_ids, helpers, replication};

const DEADLINE: Duration = Duration::from_secs(30);

async fn sync_status(http: &reqwest::Client, cluster: &TestCluster, node: usize) -> Value {
    http.get(format!("{}/api/v0/p2p/sync/status", cluster.api_url(node)))
        .send()
        .await
        .expect("sync status transport")
        .error_for_status()
        .expect("sync status response")
        .json()
        .await
        .expect("sync status JSON")
}

fn counter(status: &Value, name: &str) -> u64 {
    status[name]
        .as_u64()
        .unwrap_or_else(|| panic!("missing sync counter {name}: {status}"))
}

async fn wait_for_public_control(receiver: &DefraClient, document: &str) {
    tokio::time::timeout(DEADLINE, async {
        loop {
            let value = receiver
                .query("query { Note { _docID title } }")
                .expect("public transport control query");
            let rows = value["Note"].as_array().expect("public Note array");
            if rows.len() == 1 && rows[0]["_docID"] == document {
                assert_eq!(rows[0]["title"], "transport control");
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("public data must reach the connected non-replicator");
}

/// Ordinary peers need document access; explicitly trusted replicators may store
/// protected blocks, but that trust does not grant their identities query access.
#[tokio::test]
#[serial_test::serial]
async fn native_ungranted_peer_is_filtered_until_explicitly_trusted_replay() {
    let owner = helpers::funded_identity();
    let peer = generate_identity(&helpers::defra_binary()).expect("distinct peer identity");
    assert_ne!(owner.did, peer.did);
    let hub = helpers::start_hub_cluster().await;
    let cluster = helpers::vera_rs_builder(&hub.node(0).rpc_url(), &owner.private_key_hex, 2, true)
        .with_node_identity(1, peer.private_key_hex.clone())
        .build()
        .await
        .expect("native ACP peer cluster");
    let source = cluster.client(0);
    let receiver = cluster.client(1);
    let policy = source
        .acp_policy_add(USER_ACP_POLICY, &owner.private_key_hex)
        .expect("create shared native policy");
    let policy_id = policy["PolicyID"]
        .as_str()
        .or_else(|| policy["policyID"].as_str())
        .expect("policy ID");
    let schema = format!(
        "{}\ntype Note {{ title: String }}",
        users_schema_with_policy(policy_id)
    );
    for node in [&source, &receiver] {
        node.schema_add_with_identity(&schema, &owner.private_key_hex)
            .expect("attach shared policy and public control schema");
    }
    let addresses = receiver.p2p_info().expect("receiver addresses");
    let address = addresses
        .as_array()
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .expect("receiver P2P address");
    source.p2p_connect(&[address]).expect("connect peers");
    for node in [&source, &receiver] {
        node.p2p_collection_add(&["Note", "User"])
            .expect("subscribe without granting replicator trust");
    }
    let replicators = source.p2p_replicator_list().expect("replicator registry");
    assert!(
        replicators.as_array().expect("replicator array").is_empty(),
        "the denied phase must not install the trusted-replicator exception"
    );

    let public = source
        .query(r#"mutation { add_Note(input: {title: "transport control"}) { _docID } }"#)
        .expect("public control creation");
    let public_id = public["add_Note"][0]["_docID"].as_str().unwrap();
    receiver
        .p2p_document_sync("Note", &[public_id])
        .expect("request public control document");
    wait_for_public_control(&receiver, public_id).await;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let before = sync_status(&http, &cluster, 0).await;

    let created = source
        .query_with_identity(
            r#"mutation { add_User(input: {name: "Alice", age: 25}) { _docID } }"#,
            &owner.private_key_hex,
        )
        .expect("protected document creation");
    let document = created["add_User"][0]["_docID"].as_str().unwrap();
    replication::wait_for_submission(&cluster, 0).await;
    let commits = commit_ids(&source, document, Some(&owner.private_key_hex));
    assert!(!commits.is_empty(), "source must hold protected history");
    assert_visibility(
        "source owner before peer request",
        &source,
        document,
        Some(&owner.private_key_hex),
        &commits,
    );
    receiver
        .p2p_document_sync("User", &[document])
        .expect("dispatch protected document request");

    // A successful sync command alone does not prove delivery or denial. The
    // source must have inspected present CAR blocks and actually filtered them.
    tokio::time::timeout(DEADLINE, async {
        loop {
            assert_visibility(
                "untrusted peer must not receive protected data, even for its owner",
                &receiver,
                document,
                Some(&owner.private_key_hex),
                &[],
            );
            let status = sync_status(&http, &cluster, 0).await;
            if counter(&status, "car_filtered_cids") > counter(&before, "car_filtered_cids") {
                assert!(
                    counter(&status, "car_present_cids") > counter(&before, "car_present_cids"),
                    "denial must inspect a real stored block"
                );
                eprintln!(
                    "untrusted native peer request: present {} -> {}, filtered {} -> {}",
                    counter(&before, "car_present_cids"),
                    counter(&status, "car_present_cids"),
                    counter(&before, "car_filtered_cids"),
                    counter(&status, "car_filtered_cids")
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("protected peer request must reach the serving ACP filter");
    assert_visibility(
        "processed unauthorized request left no protected document or history",
        &receiver,
        document,
        Some(&owner.private_key_hex),
        &[],
    );
    for key in [Some(peer.private_key_hex.as_str()), None] {
        assert_visibility("untrusted peer query", &receiver, document, key, &[]);
    }

    source
        .p2p_replicator_set_with_identity(&["User"], address, &owner.private_key_hex)
        .expect("owner explicitly trusts this peer to retain protected blocks");
    replication::wait_for_document(&receiver, document, &owner.private_key_hex, &commits).await;
    for node in [&source, &receiver] {
        assert_visibility(
            "trusted storage remains readable by document owner",
            node,
            document,
            Some(&owner.private_key_hex),
            &commits,
        );
        for key in [Some(peer.private_key_hex.as_str()), None] {
            assert_visibility(
                "replicator trust is not a document or history read grant",
                node,
                document,
                key,
                &[],
            );
        }
    }
}
