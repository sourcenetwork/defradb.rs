use std::time::Duration;

use alloy_sol_types::SolCall;
use integration_test::{generate_identity, users_schema_with_policy, DefraClient, USER_ACP_POLICY};
use keyring::FileKeyring;
use vera::vera_rs::NativeWorker;
use vera_client::{parse_policy_id, VeraClient, ACP_ADDRESS};
use vera_domain::{ConsensusPublicKey, NativeTx};
use vera_modules::acp::abi::IAcp;

use super::helpers;

#[path = "policy_generations/replication.rs"]
mod replication;

const WITHOUT_READER: &str = r#"name: test-user-policy
resources:
  - name: users
    permissions:
      - name: read
        expr: writer
      - name: update
        expr: writer
      - name: delete
        expr: writer
    relations:
      - name: writer
        types:
          - actor
"#;

async fn submit(
    client: &VeraClient,
    worker: &mut NativeWorker,
    trusted: &ConsensusPublicKey,
    call: impl SolCall,
) -> u64 {
    let wire = worker
        .prepare(ACP_ADDRESS, call.abi_encode().into())
        .expect("prepare policy administration")
        .to_vec();
    let hash = NativeTx::decode_wire(&wire).unwrap().tx_id().0;
    client.send_native_tx(&wire).await.unwrap();
    let proof = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(proof) = client.read_receipt(hash, trusted).await.unwrap() {
                return proof;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("certified policy administration receipt");
    assert!(worker.acknowledge(&proof, trusted).unwrap().success());
    proof.revision.height
}

fn commit_ids(node: &DefraClient, document: &str, key: Option<&str>) -> Vec<String> {
    let query = format!(r#"query {{ _commits(docID: "{document}") {{ cid }} }}"#);
    let result = match key {
        Some(key) => node.query_with_identity(&query, key),
        None => node.query(&query),
    }
    .expect("query protected commit history");
    let mut ids: Vec<_> = result["_commits"]
        .as_array()
        .expect("commit array")
        .iter()
        .map(|commit| commit["cid"].as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    ids
}

fn assert_visibility(
    phase: &str,
    node: &DefraClient,
    document: &str,
    key: Option<&str>,
    expected_commits: &[String],
) {
    let query = "query { User { _docID name } }";
    let result = match key {
        Some(key) => node.query_with_identity(query, key),
        None => node.query(query),
    }
    .expect("query protected collection");
    let users = result["User"].as_array().expect("User array");
    if expected_commits.is_empty() {
        assert!(
            users.is_empty(),
            "{phase}: revoked or anonymous reader sees a document"
        );
    } else {
        assert_eq!(users.len(), 1, "{phase}: allowed document");
        assert_eq!(users[0]["_docID"], document);
        assert_eq!(users[0]["name"], "Alice");
    }
    assert_eq!(
        commit_ids(node, document, key),
        expected_commits,
        "{phase}: commit visibility"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn native_protected_collection_keeps_revocations_across_relation_recreation() {
    exercise_policy_generations(false).await;
}

/// Certified revocation also governs replicated data and persisted commit history.
#[tokio::test]
#[serial_test::serial]
async fn native_replicated_collection_keeps_revocations_across_relation_recreation_and_restart() {
    exercise_policy_generations(true).await;
}

async fn exercise_policy_generations(replicated: bool) {
    let hub = helpers::start_hub_cluster().await;
    let url = hub.node(0).rpc_url();
    let trusted = *vera_harness::cluster::KeySet::builder()
        .nodes(1)
        .seed(0)
        .build()
        .unwrap()
        .epoch_info()
        .output
        .public()
        .public();
    let client = VeraClient::new(&url);
    let root = tempfile::tempdir().unwrap();
    let keyring = FileKeyring::open(root.path().join("keys"), b"test-password").unwrap();
    let mut admin = NativeWorker::open(&root.path().join("admin"), &keyring, 9001).unwrap();
    let created = submit(
        &client,
        &mut admin,
        &trusted,
        IAcp::createPolicyCall {
            policy: USER_ACP_POLICY.as_bytes().to_vec().into(),
            marshalType: 1,
        },
    )
    .await;
    let page = client
        .read_policy_page(None, 2, created, &trusted)
        .await
        .expect("certified created policy");
    assert!(page.continuation.is_none());
    assert_eq!(page.records.len(), 1);
    let policy = &page.records[0];
    assert_eq!(policy.metadata.owner_did, admin.did());
    assert_eq!(policy.metadata.tx_signer, admin.did());
    assert_eq!(policy.raw_policy, USER_ACP_POLICY);
    let original_reader = policy.relations.generation("users", "reader").unwrap();
    let policy_id = &policy.policy.id;
    let policy_hash = parse_policy_id(policy_id).unwrap();

    let alice = helpers::funded_identity();
    let bob = generate_identity(&helpers::defra_binary()).expect("Bob identity");
    assert_ne!(admin.did(), alice.did);
    let mut builder = helpers::vera_rs_builder(
        &url,
        &alice.private_key_hex,
        if replicated { 2 } else { 1 },
        replicated,
    );
    if replicated {
        builder = builder.with_store("badger").with_file_keyring();
    }
    let mut cluster = builder.build().await.expect("build protected nodes");
    let nodes: Vec<_> = (0..cluster.len())
        .map(|index| cluster.client(index))
        .collect();
    for node in &nodes {
        node.schema_add_with_identity(&users_schema_with_policy(policy_id), &alice.private_key_hex)
            .expect("attach policy to collection");
    }
    if replicated {
        replication::connect(&nodes, &alice.private_key_hex);
    }
    let node = &nodes[0];
    let created = node
        .query_with_identity(
            r#"mutation { add_User(input: {name: "Alice", age: 25}) { _docID } }"#,
            &alice.private_key_hex,
        )
        .expect("create protected document");
    let document = created["add_User"][0]["_docID"].as_str().unwrap();
    let owner = Some(alice.private_key_hex.as_str());
    let reader = Some(bob.private_key_hex.as_str());
    let commits = commit_ids(node, document, owner);
    assert!(!commits.is_empty());
    replication::wait_for_submission(&cluster, 0).await;
    if replicated {
        replication::wait_for_document(&nodes[1], document, &alice.private_key_hex, &commits).await;
    }
    for node in &nodes {
        assert_visibility("owner", node, document, owner, &commits);
        assert_visibility("reader denied", node, document, reader, &[]);
        assert_visibility("anonymous denied", node, document, None, &[]);
    }
    node.acp_relationship_add("User", document, "reader", &bob.did, &alice.private_key_hex)
        .expect("grant reader");
    replication::wait_for_submission(&cluster, 0).await;
    for node in &nodes {
        assert_visibility("reader granted", node, document, reader, &commits);
    }

    for definition in [WITHOUT_READER, USER_ACP_POLICY] {
        let edited = submit(
            &client,
            &mut admin,
            &trusted,
            IAcp::editPolicyCall {
                policyId: policy_hash,
                policy: definition.as_bytes().to_vec().into(),
                marshalType: 1,
            },
        )
        .await;
        let policy = client
            .read_policy(policy_hash, edited, &trusted)
            .await
            .unwrap()
            .value
            .expect("edited policy remains live");
        assert_eq!(policy.raw_policy, definition);
        if definition == WITHOUT_READER {
            assert_eq!(policy.relations.generation("users", "reader"), None);
        } else {
            assert!(policy.relations.generation("users", "reader").unwrap() > original_reader);
        }
        replication::wait_for_height(&cluster, edited).await;
        for node in &nodes {
            assert_visibility(definition, node, document, reader, &[]);
            assert_visibility("owner", node, document, owner, &commits);
            assert_visibility("anonymous denied", node, document, None, &[]);
        }
        if replicated && definition == USER_ACP_POLICY {
            cluster
                .restart_node(1, Duration::from_secs(30))
                .await
                .unwrap();
            replication::wait_for_height(&cluster, edited).await;
            // The same on-disk CIDs remain readable, but the retired grant must not revive.
            for node in &nodes {
                assert_visibility("recreated after restart", node, document, reader, &[]);
                assert_visibility("owner after restart", node, document, owner, &commits);
                assert_visibility("anonymous after restart", node, document, None, &[]);
            }
        }
    }

    // In the replicated case, exercise authenticated writes from the restarted receiver too.
    let writer_index = usize::from(replicated);
    nodes[writer_index]
        .acp_relationship_add("User", document, "reader", &bob.did, &alice.private_key_hex)
        .expect("grant reader in the new generation");
    replication::wait_for_submission(&cluster, writer_index).await;
    for node in &nodes {
        assert_visibility("reader granted", node, document, reader, &commits);
    }
    if replicated {
        nodes[1]
            .acp_relationship_delete("User", document, "reader", &bob.did, &alice.private_key_hex)
            .expect("revoke reader from restarted receiver");
        replication::wait_for_submission(&cluster, 1).await;
        for node in &nodes {
            assert_visibility("explicit revocation", node, document, reader, &[]);
            assert_visibility("owner after revocation", node, document, owner, &commits);
        }
        node.acp_relationship_add("User", document, "reader", &bob.did, &alice.private_key_hex)
            .expect("restore current grant before policy retirement");
        replication::wait_for_submission(&cluster, 0).await;
        for node in &nodes {
            assert_visibility("current grant", node, document, reader, &commits);
        }
    }
    let retired = submit(
        &client,
        &mut admin,
        &trusted,
        IAcp::deletePolicyCall {
            policyId: policy_hash,
        },
    )
    .await;
    assert!(client
        .read_policy(policy_hash, retired, &trusted)
        .await
        .unwrap()
        .value
        .is_none());
    replication::wait_for_height(&cluster, retired).await;
    for node in &nodes {
        for identity in [owner, reader, None] {
            assert_visibility("policy retired", node, document, identity, &[]);
        }
    }
    if replicated {
        cluster
            .restart_node(1, Duration::from_secs(30))
            .await
            .unwrap();
        replication::wait_for_height(&cluster, retired).await;
        for node in &nodes {
            for identity in [owner, reader, None] {
                assert_visibility("retired after restart", node, document, identity, &[]);
            }
        }
    }
    drop(hub);
    let query = format!(r#"query {{ _commits(docID: "{document}") {{ cid }} }}"#);
    for node in &nodes {
        let error = node
            .query_with_identity(&query, &alice.private_key_hex)
            .expect_err("unavailable certified evidence must not become empty history");
        assert!(error
            .to_string()
            .contains("unable to verify access to commit history"));
    }
}
