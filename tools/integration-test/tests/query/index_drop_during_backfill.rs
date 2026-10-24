//! #2233: dropping an index while its backfill runs leaves no action behind.
//! Go still records the drop as a failed backfill
//! (sourcenetwork/defradb#5061), so this runs against the Rust node only.

use integration_test::TestCluster;
use serde_json::{json, Value};

async fn actions(api: &str) -> Value {
    reqwest::get(format!("{api}/api/v0/actions"))
        .await
        .expect("list actions request")
        .json()
        .await
        .expect("list actions response")
}

#[tokio::test]
async fn rust_index_drop_during_backfill_leaves_no_action() {
    let _root = integration_test::workspace_root();
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let node = cluster.client(0);
    let api = cluster.api_url(0);
    node.schema_add("type User { name: String age: Int }")
        .expect("add User schema");
    for batch in 0..6 {
        let users = (0..500)
            .map(|i| format!("{{name: \"u{batch}_{i}\", age: {i}}}"))
            .collect::<Vec<_>>()
            .join(", ");
        node.query(&format!(
            "mutation {{ add_User(input: [{users}]) {{ _docID }} }}"
        ))
        .expect("create users");
    }

    let client = reqwest::Client::new();
    let create = tokio::spawn({
        let client = client.clone();
        let url = format!("{api}/api/v0/collections/User/indexes");
        async move {
            client
                .post(url)
                .json(&json!({"Name": "by_age", "Fields": [{"Name": "age"}]}))
                .send()
                .await
                .expect("create index request")
                .error_for_status()
                .expect("create index status")
        }
    });
    while actions(&api).await == json!([]) {
        assert!(
            !create.is_finished(),
            "the backfill finished before the drop could race it"
        );
        tokio::task::yield_now().await;
    }
    client
        .delete(format!("{api}/api/v0/collections/User/indexes/by_age"))
        .send()
        .await
        .expect("drop index request")
        .error_for_status()
        .expect("drop index status");
    create.await.expect("create index task");

    assert_eq!(actions(&api).await, json!([]));
    let indexes = node.index_list(Some("User")).expect("index_list User");
    assert!(
        indexes.as_array().is_some_and(|list| list.is_empty())
            || indexes
                .as_object()
                .is_some_and(|map| map.values().all(|list| list == &json!([]))),
        "by_age should be gone: {indexes}"
    );
}
