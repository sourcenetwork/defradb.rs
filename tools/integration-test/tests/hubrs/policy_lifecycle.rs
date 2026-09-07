use integration_test::{generate_identity, users_schema_with_policy, USER_ACP_POLICY};

use super::helpers;

/// Native Vera grants and revocation apply to document and commit-history queries.
#[tokio::test]
#[serial_test::serial]
async fn rust_hubrs_policy_lifecycle() {
    let binary = helpers::defra_binary();
    let alice = helpers::funded_identity();

    let hub = helpers::start_hub_cluster().await;
    let hub_rpc_url = hub.node(0).rpc_url();

    let cluster =
        helpers::build_defra_with_hub_rs(&hub_rpc_url, &alice.private_key_hex, 1, false).await;
    let node = cluster.client(0);

    let bob = generate_identity(&binary).expect("Bob identity");

    let policy_result = node
        .acp_policy_add(USER_ACP_POLICY, &alice.private_key_hex)
        .expect("create policy");
    let policy_id = policy_result["PolicyID"]
        .as_str()
        .or_else(|| policy_result["policyID"].as_str())
        .expect("PolicyID")
        .to_string();

    let exists = helpers::policy_exists(&hub_rpc_url, &policy_id).await;
    assert!(exists, "created policy must have a verified record");

    // Deploy schema with policy
    let schema = users_schema_with_policy(&policy_id);
    node.schema_add_with_identity(&schema, &alice.private_key_hex)
        .expect("add schema");

    // Create a document as Alice
    let data = node
        .query_with_identity(
            r#"mutation { add_User(input: {name: "Alice", age: 25}) { _docID name } }"#,
            &alice.private_key_hex,
        )
        .expect("create user");
    let doc_id = data["add_User"][0]["_docID"].as_str().expect("_docID");

    let commits_query = format!(r#"query {{ _commits(docID: "{doc_id}") {{ cid }} }}"#);
    let owner_commits = node
        .query_with_identity(&commits_query, &alice.private_key_hex)
        .expect("owner commits");
    let owner_cids: Vec<_> = owner_commits["_commits"]
        .as_array()
        .expect("owner commit array")
        .iter()
        .map(|commit| commit["cid"].as_str().expect("commit CID").to_owned())
        .collect();
    assert!(!owner_cids.is_empty(), "owner should see document history");
    let bob_commits = node
        .query_with_identity(&commits_query, &bob.private_key_hex)
        .expect("commits before grant");
    assert!(bob_commits["_commits"].as_array().unwrap().is_empty());
    let anonymous_commits = node.query(&commits_query).expect("anonymous commits");
    assert!(anonymous_commits["_commits"].as_array().unwrap().is_empty());

    let bob_before = node
        .query_with_identity("query { User { _docID name } }", &bob.private_key_hex)
        .expect("Bob query before grant");
    assert_eq!(
        bob_before["User"].as_array().unwrap().len(),
        0,
        "Bob should see 0 docs before grant"
    );

    // Grant Bob reader (on-chain relationship tx via EVM)
    node.acp_relationship_add("User", doc_id, "reader", &bob.did, &alice.private_key_hex)
        .expect("grant Bob reader");

    // Bob can now read
    let bob_after = node
        .query_with_identity("query { User { _docID name } }", &bob.private_key_hex)
        .expect("Bob query after grant");
    let bob_users = bob_after["User"].as_array().unwrap();
    assert_eq!(bob_users.len(), 1, "Bob should see 1 doc after grant");
    assert_eq!(bob_users[0]["name"], "Alice");

    let bob_commits = node
        .query_with_identity(&commits_query, &bob.private_key_hex)
        .expect("commits after grant");
    let bob_cids: Vec<_> = bob_commits["_commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|commit| commit["cid"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(bob_cids, owner_cids);

    node.acp_relationship_delete("User", doc_id, "reader", &bob.did, &alice.private_key_hex)
        .expect("revoke Bob reader");

    let revoked_commits = node
        .query_with_identity(&commits_query, &bob.private_key_hex)
        .expect("commits after revoke");
    assert!(revoked_commits["_commits"].as_array().unwrap().is_empty());

    let bob_revoked = node
        .query_with_identity("query { User { _docID name } }", &bob.private_key_hex)
        .expect("Bob query after revoke");
    assert_eq!(
        bob_revoked["User"].as_array().unwrap().len(),
        0,
        "Bob should see 0 docs after revoke"
    );
}
