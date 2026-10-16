//! Cursor pagination over an indexed, policy-protected collection (#1990).
//!
//! The first forward page bounds the index scan at `page_size + 1`. Documents
//! the caller cannot read are dropped above the scan, so a bounded scan whose
//! leading entries are all private returns a short or empty page.

use integration_test::{generate_identity, TestCluster};
use serde_json::Value;

const POLICY: &str = r#"
description: cursor pagination over a protected collection
name: test
resources:
  - name: users
    permissions:
      - name: read
        expr: reader
      - name: update
        expr: updater
      - name: delete
        expr: deleter
    relations:
      - name: reader
        types:
          - actor
      - name: updater
        types:
          - actor
      - name: deleter
        types:
          - actor
"#;

#[tokio::test]
async fn rust_acp_cursor_first_page_skips_unreadable_index_entries() {
    let cluster = TestCluster::builder()
        .rust_nodes(1)
        .with_acp_local()
        .build()
        .await
        .unwrap();
    let node = cluster.client(0);
    let alice = generate_identity(node.binary_path()).expect("alice");
    let policy = node
        .acp_policy_add(POLICY, &alice.private_key_hex)
        .expect("add policy");
    let policy_id = policy["PolicyID"]
        .as_str()
        .or_else(|| policy["policyID"].as_str())
        .expect("policy id");
    node.schema_add_with_identity(
        &format!(
            r#"type Users @policy(id: "{policy_id}", resource: "users") {{ name: String  age: Int @index }}"#
        ),
        &alice.private_key_hex,
    )
    .expect("add schema");

    // The lowest ages are private to Alice, so they lead the index.
    for (name, age) in [("p1", 10), ("p2", 20), ("p3", 30)] {
        node.query_with_identity(
            &format!(
                r#"mutation {{ add_Users(input: {{name: "{name}", age: {age}}}) {{ _docID }} }}"#
            ),
            &alice.private_key_hex,
        )
        .expect("create private doc");
    }
    for (name, age) in [("public-a", 40), ("public-b", 50)] {
        node.query(&format!(
            r#"mutation {{ add_Users(input: {{name: "{name}", age: {age}}}) {{ _docID }} }}"#
        ))
        .expect("create public doc");
    }

    let result: Value = node
        .query(
            r#"{ _cursor {
                Users(first: 2, order: [{age: ASC}]) { name }
                _pageInfo { hasNext }
            } }"#,
        )
        .expect("anonymous cursor query");

    let names: Vec<&str> = result["_cursor"]["Users"]
        .as_array()
        .expect("_cursor.Users is an array")
        .iter()
        .filter_map(|user| user["name"].as_str())
        .collect();
    assert_eq!(names, ["public-a", "public-b"]);
    assert_eq!(
        result["_cursor"]["_pageInfo"]["hasNext"],
        Value::Bool(false)
    );
}
