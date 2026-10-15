//! Introspection through the schema cache (#1855).
//!
//! The node caches built introspection schemas keyed by schema epoch and, in
//! full mode, by view content. These tests pin the two visibility guarantees
//! the cache must never bend: a schema added inside a transaction is visible
//! to introspection in that transaction, and never leaks into introspection
//! outside it until commit — including when the committed schema's cache
//! entry is warm on both sides of the transaction.

use integration_test::TestCluster;

fn type_name(result: &serde_json::Value) -> Option<&str> {
    result["__type"]["name"].as_str()
}

async fn txn_schema_add_is_scoped_under_warm_cache(cluster: TestCluster) {
    let client = cluster.client(0);
    let api_url = cluster.api_url(0);

    client
        .schema_add("type CacheUser { name: String }")
        .expect("schema add failed");

    // Warm the committed view's cache entry and prove it serves the schema.
    for _ in 0..3 {
        let result = client
            .query(r#"{ __type(name: "CacheUser") { name } }"#)
            .expect("introspection failed");
        assert_eq!(type_name(&result), Some("CacheUser"));
    }

    let tx_id = client.tx_create().expect("tx_create failed");
    let resp = reqwest::Client::new()
        .post(format!("{}/api/v0/collections", api_url))
        .header("x-defradb-tx", &tx_id)
        .body("type CacheWidget { name: String }")
        .send()
        .await
        .expect("schema add in tx request failed");
    assert!(
        resp.status().is_success(),
        "schema add in tx failed: {}",
        resp.status()
    );

    // Inside the transaction: the uncommitted schema must be introspectable —
    // the warm committed entry must not be served for the transaction's view.
    let in_tx = client
        .query_with_tx(r#"{ __type(name: "CacheWidget") { name } }"#, &tx_id)
        .expect("introspection in tx failed");
    assert_eq!(
        type_name(&in_tx),
        Some("CacheWidget"),
        "uncommitted schema invisible to introspection in its own transaction"
    );

    // Outside: the transaction's view must not have polluted the committed
    // cache entry, warm or rebuilt.
    for _ in 0..2 {
        let outside = client
            .query(r#"{ __type(name: "CacheWidget") { name } }"#)
            .expect("introspection outside tx failed");
        assert_eq!(
            type_name(&outside),
            None,
            "uncommitted schema leaked into introspection outside the transaction"
        );
    }

    client.tx_commit(&tx_id).expect("tx_commit failed");

    // Commit bumps the schema epoch: the stale warm entry must not be served.
    let after = client
        .query(r#"{ __type(name: "CacheWidget") { name } }"#)
        .expect("introspection after commit failed");
    assert_eq!(
        type_name(&after),
        Some("CacheWidget"),
        "committed schema invisible: stale cached introspection schema served"
    );
}

#[tokio::test]
async fn rust_txn_schema_add_is_scoped_under_warm_cache() {
    let _root = integration_test::workspace_root();
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    txn_schema_add_is_scoped_under_warm_cache(cluster).await;
}
