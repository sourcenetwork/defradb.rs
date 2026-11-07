//! #2233: dropping an index while its backfill runs leaves no action behind.
//! Go still records the drop as a failed backfill
//! (sourcenetwork/defradb#5061), so this runs against the Rust node only.

use integration_test::TestCluster;
use serde_json::{json, Value};

async fn actions(api: &str) -> Value {
    net::http::Client::new(edge::testing::edge())
        .get(format!("{api}/api/v0/actions"))
        .send()
        .await
        .expect("list actions request")
        .json()
        .await
        .expect("list actions response")
}

#[nuclei::test]
async fn rust_index_drop_during_backfill_leaves_no_action() {
    edge::compat(async {
        let _root = integration_test::workspace_root();
        let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
        let node = cluster.client(0);
        let api = cluster.api_url(0);
        node.schema_add("type User { name: String age: Int }")
            .expect("add User schema");
        for batch in 0..12 {
            let users = (0..500)
                .map(|i| format!("{{name: \"u{batch}_{i}\", age: {i}}}"))
                .collect::<Vec<_>>()
                .join(", ");
            node.query(&format!(
                "mutation {{ add_User(input: [{users}]) {{ _docID }} }}"
            ))
            .expect("create users");
        }

        let client = net::http::Client::new(edge::testing::edge());
        let drop_url = format!("{api}/api/v0/collections/User/indexes/by_age");
        // A fast runner can finish the backfill before the drop lands, so race
        // again rather than fail on timing.
        let mut raced = false;
        for _ in 0..5 {
            let create = nuclei::spawn({
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
            // Drop as soon as the index exists: a drop that waited to see the
            // backfill running could miss it entirely on a single worker.
            loop {
                let finished = create.is_finished();
                let dropped = client
                    .delete(&drop_url)
                    .send()
                    .await
                    .expect("drop index request");
                if dropped.status().is_success() {
                    raced = !finished;
                    break;
                }
                nuclei::task::yield_now().await;
            }
            create.await;
            if raced {
                break;
            }
        }
        assert!(
            raced,
            "the backfill finished before every drop could race it"
        );

        assert_eq!(actions(api).await, json!([]));
        let indexes = node.index_list(Some("User")).expect("index_list User");
        assert!(
            indexes.as_array().is_some_and(|list| list.is_empty())
                || indexes
                    .as_object()
                    .is_some_and(|map| map.values().all(|list| list == &json!([]))),
            "by_age should be gone: {indexes}"
        );
    })
    .await;
}
