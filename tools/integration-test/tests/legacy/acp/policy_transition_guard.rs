use integration_test::{
    for_each_runtime, generate_identity, users_schema_with_policy, TestCluster, USER_ACP_POLICY,
};

/// 02-28: Policy transition guard — revoking access before a schema policy change
/// correctly blocks the formerly-authorized user.
///
/// This tests the guard activation sequence: grant → revoke → verify denial.
/// It simulates a "policy transition" scenario where access is explicitly revoked
/// and then the revocation is verified to be effective.
async fn policy_transition_guard_test(cluster: TestCluster) {
    let node = cluster.client(0);
    let binary = node.binary_path().to_path_buf();

    let alice = generate_identity(&binary).expect("Alice identity");
    let bob = generate_identity(&binary).expect("Bob identity");
    let carol = generate_identity(&binary).expect("Carol identity");

    let policy = node
        .acp_policy_add(USER_ACP_POLICY, &alice.private_key_hex)
        .expect("add policy");
    let policy_id = policy["PolicyID"]
        .as_str()
        .or_else(|| policy["policyID"].as_str())
        .expect("missing PolicyID");
    let schema = users_schema_with_policy(policy_id);
    node.schema_add_with_identity(&schema, &alice.private_key_hex)
        .expect("add schema");

    let data = node
        .query_with_identity(
            r#"mutation { add_User(input: {name: "Guarded", age: 5}) { _docID } }"#,
            &alice.private_key_hex,
        )
        .expect("create doc");
    let doc_id = data["add_User"][0]["_docID"]
        .as_str()
        .expect("_docID")
        .to_string();

    let count = |key: &str| -> usize {
        node.query_with_identity("query { User { _docID name } }", key)
            .expect("query")["User"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0)
    };

    // Phase 1: grant Bob and Carol access
    node.acp_relationship_add("User", &doc_id, "reader", &bob.did, &alice.private_key_hex)
        .expect("grant Bob reader");
    node.acp_relationship_add(
        "User",
        &doc_id,
        "writer",
        &carol.did,
        &alice.private_key_hex,
    )
    .expect("grant Carol writer");

    assert_eq!(count(&bob.private_key_hex), 1, "Bob sees doc after grant");
    assert_eq!(
        count(&carol.private_key_hex),
        1,
        "Carol sees doc after grant"
    );

    // Phase 2: transition guard — revoke both before policy change
    node.acp_relationship_delete("User", &doc_id, "reader", &bob.did, &alice.private_key_hex)
        .expect("revoke Bob reader");
    node.acp_relationship_delete(
        "User",
        &doc_id,
        "writer",
        &carol.did,
        &alice.private_key_hex,
    )
    .expect("revoke Carol writer");

    // Guard must be effective immediately — both must now see 0 documents
    assert_eq!(
        count(&bob.private_key_hex),
        0,
        "Bob must be blocked immediately after revocation (transition guard)"
    );
    assert_eq!(
        count(&carol.private_key_hex),
        0,
        "Carol must be blocked immediately after revocation (transition guard)"
    );

    // Phase 3: verify guard does not affect owner (Alice) — she still sees the doc
    assert_eq!(count(&alice.private_key_hex), 1, "Alice (owner) unaffected");

    // Phase 4: re-grant Bob only — Carol remains blocked
    node.acp_relationship_add("User", &doc_id, "reader", &bob.did, &alice.private_key_hex)
        .expect("re-grant Bob reader");

    assert_eq!(
        count(&bob.private_key_hex),
        1,
        "Bob regains access after re-grant"
    );
    assert_eq!(
        count(&carol.private_key_hex),
        0,
        "Carol remains blocked — transition guard still active for her"
    );

    // Phase 5: verify Carol cannot write (mutation denial after guard)
    let carol_write = node.query_with_identity(
        &format!(
            r#"mutation {{ update_User(docID: "{}", input: {{age: 99}}) {{ _docID }} }}"#,
            doc_id
        ),
        &carol.private_key_hex,
    );
    match carol_write {
        Err(_) => {}
        Ok(val) => {
            let updated = val["update_User"].as_array().map(|a| a.len()).unwrap_or(0);
            assert_eq!(
                updated, 0,
                "Carol must not update after transition guard revoked her writer access"
            );
        }
    }

    // Document content is still intact
    let final_read = node
        .query_with_identity("query { User { _docID name age } }", &alice.private_key_hex)
        .expect("final read");
    let users = final_read["User"].as_array().expect("User array");
    assert_eq!(
        users.len(),
        1,
        "document must survive all transition operations"
    );
    assert_eq!(users[0]["name"], "Guarded", "document content unchanged");
    assert_eq!(
        users[0]["age"], 5,
        "age unchanged after Carol's blocked write"
    );
}

for_each_runtime!(policy_transition_guard, policy_transition_guard_test, .with_acp_local());
