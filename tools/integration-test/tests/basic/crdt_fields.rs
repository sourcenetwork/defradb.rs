//! LWW and counter fields through every client surface: GraphQL, the HTTP
//! document routes, and the CLI, plus restart and two-node convergence.

use std::time::Duration;

use integration_test::{extract_p2p_addr, poll_until, DefraClient, TestCluster, P2P_TIMEOUT};
use serde_json::Value;

const SDL: &str = "type Tally {
    name: String
    hits: Int @crdt(type: pcounter)
    balance: Float @crdt(type: pncounter)
}";

const START: &str = r#"{"name": "first", "hits": 2, "balance": 1.5}"#;
const EDIT: &str = r#"{"name": "second", "hits": 3, "balance": -4}"#;

async fn single_node() -> TestCluster {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    cluster.client(0).schema_add(SDL).expect("schema add");
    cluster
}

fn try_tally(node: &DefraClient) -> Option<Value> {
    let data = node
        .query("query { Tally { _docID name hits balance } }")
        .ok()?;
    match data["Tally"].as_array()?.as_slice() {
        [row] => Some(row.clone()),
        _ => None,
    }
}

fn tally(node: &DefraClient) -> Value {
    try_tally(node).expect("exactly one Tally document")
}

fn doc_id(node: &DefraClient) -> String {
    tally(node)["_docID"].as_str().expect("_docID").to_string()
}

fn assert_tally(row: &Value, name: &str, hits: i64, balance: f64) {
    assert_eq!(row["name"], name, "{row}");
    assert_eq!(row["hits"].as_i64(), Some(hits), "{row}");
    assert_eq!(row["balance"].as_f64(), Some(balance), "{row}");
}

fn update_gql(node: &DefraClient, id: &str, input: &str) {
    node.query(&format!(
        r#"mutation {{ update_Tally(docID: "{id}", input: {input}) {{ _docID }} }}"#
    ))
    .expect("update_Tally");
}

#[nuclei::test]
async fn graphql_updates_replace_lww_and_add_to_counters() {
    edge::compat(async move {
        let cluster = single_node().await;
        let node = cluster.client(0);
        node.query(
            r#"mutation { add_Tally(input: {name: "first", hits: 2, balance: 1.5}) { _docID } }"#,
        )
        .expect("add_Tally");
        let id = doc_id(&node);

        update_gql(&node, &id, r#"{name: "second", hits: 3, balance: -4}"#);
        assert_tally(&tally(&node), "second", 5, -2.5);

        update_gql(&node, &id, "{hits: 0}");
        assert_tally(&tally(&node), "second", 5, -2.5);
    })
    .await;
}

#[nuclei::test]
async fn graphql_rejects_a_negative_pcounter_increment() {
    edge::compat(async move {
        let cluster = single_node().await;
        let node = cluster.client(0);
        node.query(
            r#"mutation { add_Tally(input: {name: "first", hits: 2, balance: 1.5}) { _docID } }"#,
        )
        .expect("add_Tally");
        let id = doc_id(&node);

        let error = node
            .query_expect_error(&format!(
                r#"mutation {{ update_Tally(docID: "{id}", input: {{hits: -1}}) {{ _docID }} }}"#
            ))
            .expect("negative increment error");
        assert!(error.contains("value cannot be negative"), "{error}");
        assert_tally(&tally(&node), "first", 2, 1.5);
    })
    .await;
}

#[nuclei::test]
async fn http_rejects_a_negative_pcounter_increment() {
    edge::compat(async move {
        let cluster = single_node().await;
        let node = cluster.client(0);
        node.query(
            r#"mutation { add_Tally(input: {name: "first", hits: 2, balance: 1.5}) { _docID } }"#,
        )
        .expect("add_Tally");
        let id = doc_id(&node);

        let response = net::http::Client::new(edge::testing::edge())
            .patch(format!(
                "{}/api/v0/collections/Tally/document/{id}",
                cluster.api_url(0)
            ))
            .json(&serde_json::json!({"hits": -1}))
            .send()
            .await
            .expect("PATCH document");
        assert!(!response.status().is_success(), "{}", response.status());
        let body = response.text().await.expect("error body");
        assert!(body.contains("value cannot be negative"), "{body}");
        assert_tally(&tally(&node), "first", 2, 1.5);
    })
    .await;
}

#[nuclei::test]
async fn cli_rejects_a_negative_pcounter_increment() {
    edge::compat(async move {
        let cluster = single_node().await;
        let node = cluster.client(0);
        node.collection_create("Tally", START)
            .expect("document add");
        let id = doc_id(&node);

        let error = node
            .collection_update("Tally", &id, r#"{"hits": -1}"#)
            .expect_err("negative increment");
        assert!(
            error.to_string().contains("value cannot be negative"),
            "{error}"
        );
        assert_tally(&tally(&node), "first", 2, 1.5);
    })
    .await;
}

#[nuclei::test]
async fn float_counters_add_in_f64() {
    edge::compat(async move {
        let cluster = single_node().await;
        let node = cluster.client(0);
        node.query(
            r#"mutation { add_Tally(input: {name: "first", hits: 0, balance: 0}) { _docID } }"#,
        )
        .expect("add_Tally");
        let id = doc_id(&node);

        update_gql(&node, &id, "{balance: 0.1}");
        update_gql(&node, &id, "{balance: 0.2}");
        assert_eq!(
            tally(&node)["balance"].as_f64(),
            Some(0.30000000000000004),
            "f64 sum"
        );
    })
    .await;
}

#[nuclei::test]
async fn http_document_updates_replace_lww_and_add_to_counters() {
    edge::compat(async move {
        let cluster = single_node().await;
        let node = cluster.client(0);
        let api = format!("{}/api/v0/collections/Tally", cluster.api_url(0));
        let http = net::http::Client::new(edge::testing::edge());

        let created = http
            .post(&api)
            .json(&serde_json::from_str::<Value>(START).unwrap())
            .send()
            .await
            .expect("POST document");
        assert!(created.status().is_success(), "{}", created.status());
        let id = doc_id(&node);

        let updated = http
            .patch(format!("{api}/document/{id}"))
            .json(&serde_json::from_str::<Value>(EDIT).unwrap())
            .send()
            .await
            .expect("PATCH document");
        assert!(updated.status().is_success(), "{}", updated.status());

        let fetched: Value = http
            .get(format!("{api}/document/{id}"))
            .send()
            .await
            .expect("GET document")
            .json()
            .await
            .expect("document JSON");
        assert_tally(&fetched, "second", 5, -2.5);
        assert_tally(&tally(&node), "second", 5, -2.5);
    })
    .await;
}

#[nuclei::test]
async fn cli_document_updates_replace_lww_and_add_to_counters() {
    edge::compat(async move {
        let cluster = single_node().await;
        let node = cluster.client(0);
        node.collection_create("Tally", START)
            .expect("document add");
        let id = doc_id(&node);

        node.collection_update("Tally", &id, EDIT)
            .expect("document update");

        assert_tally(
            &node.collection_get("Tally", &id).expect("document get"),
            "second",
            5,
            -2.5,
        );
        assert_tally(&tally(&node), "second", 5, -2.5);
    })
    .await;
}

#[nuclei::test]
async fn crdt_fields_keep_their_values_and_kinds_across_restart() {
    edge::compat(async move {
        let mut cluster = TestCluster::builder()
            .rust_nodes(1)
            .with_store("regolith")
            .build()
            .await
            .unwrap();
        let node = cluster.client(0);
        node.schema_add(SDL).expect("schema add");
        node.query(
            r#"mutation { add_Tally(input: {name: "first", hits: 2, balance: 1.5}) { _docID } }"#,
        )
        .expect("add_Tally");
        let id = doc_id(&node);
        update_gql(&node, &id, r#"{name: "second", hits: 3, balance: -4}"#);

        cluster
            .restart_node(0, Duration::from_secs(60))
            .await
            .expect("restart");
        let node = cluster.client(0);
        assert_tally(&tally(&node), "second", 5, -2.5);

        update_gql(&node, &id, r#"{name: "third", hits: 1, balance: 2.5}"#);
        assert_tally(&tally(&node), "third", 6, 0.0);
    })
    .await;
}

#[nuclei::test]
async fn concurrent_updates_on_two_nodes_converge() {
    edge::compat(async move {
        let cluster = TestCluster::builder()
            .rust_nodes(2)
            .with_iroh_transport()
            .build()
            .await
            .unwrap();
        for index in 0..2 {
            cluster
                .wait_for_log(index, "p2p_listening", P2P_TIMEOUT)
                .await
                .expect("P2P listener did not start");
        }
        let node0 = cluster.client(0);
        let node1 = cluster.client(1);
        let addr0 = extract_p2p_addr(&cluster, 0);
        let addr1 = extract_p2p_addr(&cluster, 1);

        // The same content gives the same docID, so both nodes start from one
        // document and edit it with no link between them.
        for node in [&node0, &node1] {
            node.schema_add(SDL).expect("schema add");
            node.p2p_collection_add(&["Tally"]).expect("collection add");
            node.query(
            r#"mutation { add_Tally(input: {name: "first", hits: 2, balance: 1.5}) { _docID } }"#,
        )
        .expect("add_Tally");
        }
        let id = doc_id(&node0);
        assert_eq!(id, doc_id(&node1), "same content, same docID");
        update_gql(&node0, &id, r#"{name: "zero", hits: 3, balance: -4}"#);
        update_gql(&node1, &id, r#"{name: "one", hits: 10, balance: 0.5}"#);

        node0.p2p_connect(&[&addr1]).expect("connect");
        node0
            .p2p_replicator_set(&["Tally"], &addr1)
            .expect("replicator 0 to 1");
        node1
            .p2p_replicator_set(&["Tally"], &addr0)
            .expect("replicator 1 to 0");

        poll_until(
            || match (try_tally(&node0), try_tally(&node1)) {
                (Some(a), Some(b)) => a == b && a["hits"].as_i64() == Some(15),
                _ => false,
            },
            P2P_TIMEOUT,
            Duration::from_millis(200),
            "nodes did not converge",
        )
        .await;
        // Equal heights: the greater CBOR bytes win, and "zero" (0x64 header)
        // beats "one" (0x63).
        assert_tally(&tally(&node0), "zero", 15, -2.0);
        assert_tally(&tally(&node1), "zero", 15, -2.0);
    })
    .await;
}
