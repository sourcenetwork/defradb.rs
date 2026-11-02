use integration_test::TestCluster;

async fn filtered_truncate_test(cluster: TestCluster) {
    let client = cluster.client(0);
    client
        .schema_add("type TruncateUser { name: String age: Int }")
        .expect("failed to add schema");
    for (name, age) in [("Alice", 30), ("Bob", 40)] {
        client
            .query(&format!(
                r#"mutation {{ add_TruncateUser(input: {{name: "{name}", age: {age}}}) {{ _docID }} }}"#
            ))
            .expect("failed to create document");
    }

    let result = client
        .query(r#"mutation { truncate_TruncateUser(filter: {age: {_eq: 30}}) }"#)
        .expect("filtered truncate failed");
    assert_eq!(result["truncate_TruncateUser"], true);

    let result = client
        .query("query { TruncateUser { name age } }")
        .expect("failed to query survivors");
    assert_eq!(
        result["TruncateUser"],
        serde_json::json!([{"name": "Bob", "age": 40}])
    );
}

#[tokio::test]
async fn rust_filtered_truncate() {
    let _root = integration_test::workspace_root();
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    filtered_truncate_test(cluster).await;
}
