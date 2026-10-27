//! Under local ACP a chunk is exactly as visible as its parent.

use integration_test::{generate_identity, TestCluster, USER_ACP_POLICY};

use super::support::{body, listed, visible, visible_hashes, CHUNKS};

fn schema(policy_id: &str) -> String {
    format!(
        r#"type Article @policy(id: "{policy_id}", resource: "users") {{
            title: String
            body: String @chunk(maxChars: 64)
        }}"#
    )
}

#[tokio::test]
async fn chunks_follow_their_parents_access() {
    let cluster = TestCluster::builder()
        .rust_nodes(1)
        .with_acp_local()
        .build()
        .await
        .unwrap();
    let node = cluster.client(0);
    let binary = node.binary_path().to_path_buf();
    let alice = generate_identity(&binary).unwrap();
    let bob = generate_identity(&binary).unwrap();
    let (alice_key, bob_key) = (alice.private_key_hex.as_str(), bob.private_key_hex.as_str());

    let policy = node.acp_policy_add(USER_ACP_POLICY, alice_key).unwrap();
    let policy_id = policy["PolicyID"]
        .as_str()
        .or_else(|| policy["policyID"].as_str())
        .unwrap()
        .to_string();
    node.schema_add_with_identity(&schema(&policy_id), alice_key)
        .unwrap();

    let data = node
        .query_with_identity(
            &format!(
                r#"mutation {{ add_Article(input: {{title: "secret", body: "{}"}}) {{ _docID }} }}"#,
                body(5, "plans")
            ),
            alice_key,
        )
        .unwrap();
    let doc_id = data["add_Article"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();

    let alice_chunks = visible_hashes(&node, Some(alice_key));
    assert!(!alice_chunks.is_empty());
    assert_eq!(alice_chunks, listed(&node, &doc_id, Some(alice_key)));
    let joined = node
        .query_with_identity(
            &format!("query {{ {CHUNKS} {{ source {{ title }} }} }}"),
            alice_key,
        )
        .unwrap();
    assert_eq!(joined[CHUNKS][0]["source"]["title"], "secret");

    assert!(visible_hashes(&node, Some(bob_key)).is_empty());
    assert!(visible_hashes(&node, None).is_empty());

    let (chunk_id, _) = visible(&node, Some(alice_key)).remove(0);
    let commits = |key: Option<&str>| {
        let query = format!(r#"query {{ _commits(docID: "{chunk_id}") {{ cid }} }}"#);
        let data = match key {
            Some(key) => node.query_with_identity(&query, key),
            None => node.query(&query),
        }
        .unwrap();
        data["_commits"].as_array().unwrap().len()
    };
    assert!(commits(Some(alice_key)) > 0);
    assert_eq!(commits(Some(bob_key)), 0);
    assert_eq!(commits(None), 0);

    // Every other read surface hides the chunks the same way.
    let read = |query: &str, key: Option<&str>| {
        match key {
            Some(key) => node.query_with_identity(query, key),
            None => node.query(query),
        }
        .unwrap()
    };
    let count = |key: Option<&str>| {
        let data = read(
            &format!("query {{ COUNT({CHUNKS}: {{limit: 1000}}) }}"),
            key,
        );
        data["COUNT"]
            .as_u64()
            .unwrap_or_else(|| panic!("no count: {data}"))
    };
    assert_eq!(count(Some(alice_key)) as usize, alice_chunks.len());
    assert_eq!(count(Some(bob_key)), 0);
    assert_eq!(count(None), 0);

    let cid = read(
        &format!(r#"query {{ _commits(docID: "{chunk_id}") {{ cid }} }}"#),
        Some(alice_key),
    )["_commits"][0]["cid"]
        .as_str()
        .unwrap()
        .to_string();
    let at_cid = |key: Option<&str>| {
        read(
            &format!(r#"query {{ {CHUNKS}(cid: "{cid}") {{ text }} }}"#),
            key,
        )[CHUNKS]
            .as_array()
            .unwrap()
            .len()
    };
    assert!(at_cid(Some(alice_key)) > 0);
    assert_eq!(at_cid(Some(bob_key)), 0);
    assert_eq!(at_cid(None), 0);

    // Explain counts rows before the permission filter, as for any collection
    // with a policy, so only check that it runs.
    let explain = read(
        &format!("query @explain(type: execute) {{ {CHUNKS} {{ text }} }}"),
        Some(bob_key),
    );
    assert!(
        explain.to_string().contains("permissionFilterNode"),
        "{explain}"
    );

    let arrivals = |key: Option<&str>| {
        read(
            &format!(
                r#"{{ _documentArrivals(collection: "{CHUNKS}") {{ entries {{ docID }} }} }}"#
            ),
            key,
        )["_documentArrivals"]["entries"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(arrivals(Some(alice_key)), alice_chunks.len());
    assert_eq!(arrivals(Some(bob_key)), 0);
    assert_eq!(arrivals(None), 0);

    let versions = |key: Option<&str>| {
        read(
            &format!("query {{ {CHUNKS} {{ _version {{ cid }} }} }}"),
            key,
        )[CHUNKS]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(versions(Some(alice_key)), alice_chunks.len());
    assert_eq!(versions(Some(bob_key)), 0);
    assert_eq!(versions(None), 0);

    node.acp_relationship_add("Article", &doc_id, "reader", &bob.did, alice_key)
        .unwrap();
    assert_eq!(visible_hashes(&node, Some(bob_key)), alice_chunks);

    node.query_with_identity(
        &format!(r#"mutation {{ delete_Article(docID: "{doc_id}") {{ _docID }} }}"#),
        alice_key,
    )
    .unwrap();
    assert!(visible_hashes(&node, Some(alice_key)).is_empty());
    assert_eq!(commits(Some(alice_key)), 0);
}
