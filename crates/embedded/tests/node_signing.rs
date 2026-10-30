//! A node built with `enable_signing()` signs what `execute` writes.

use embedded::NodeBuilder;

#[test]
fn execute_signs_with_the_node_identity() {
    workers::block_on(async {
        let node = NodeBuilder::default()
            .enable_signing()
            .build()
            .await
            .expect("node starts");
        node.add_collection("type Users { name: String }")
            .await
            .expect("collection added");

        let created = node
            .execute(r#"mutation { add_Users(input: {name: "Alice"}) { _docID } }"#)
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);

        let response = node
            .execute("query { _commits { fieldName signature { type } } }")
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let commits = response
            .data
            .as_ref()
            .and_then(|data| data["_commits"].as_array())
            .expect("_commits array");
        assert!(!commits.is_empty(), "expected commits for the write");
        assert!(
            commits.iter().all(|commit| !commit["signature"].is_null()),
            "unsigned commits: {commits:?}"
        );
        node.shutdown().await;
    })
    .unwrap()
}
