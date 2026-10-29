//! A positive counter refuses a negative value when a document is created,
//! through GraphQL, the HTTP document route and the CLI, with Go's message.

use integration_test::TestCluster;

const SDL: &str = "type Tally {
    name: String
    hits: Int @crdt(type: pcounter)
}";

const REFUSAL: &str = "value cannot be negative";

async fn single_node() -> TestCluster {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    cluster.client(0).schema_add(SDL).expect("schema add");
    cluster
}

fn assert_no_documents(cluster: &TestCluster) {
    let data = cluster
        .client(0)
        .query("query { Tally { _docID } }")
        .expect("query Tally");
    assert_eq!(data["Tally"].as_array().map(Vec::len), Some(0), "{data}");
}

#[nuclei::test]
async fn graphql_create_rejects_a_negative_pcounter() {
    edge::compat(async move {
        let cluster = single_node().await;
        let error = cluster
            .client(0)
            .query_expect_error(
                r#"mutation { add_Tally(input: {name: "first", hits: -1}) { _docID } }"#,
            )
            .expect("negative create error");
        assert!(error.contains(REFUSAL), "{error}");
        assert_no_documents(&cluster);
    })
    .await;
}

#[nuclei::test]
async fn http_create_rejects_a_negative_pcounter() {
    edge::compat(async move {
        let cluster = single_node().await;
        let response = net::http::Client::new(edge::testing::edge())
            .post(format!("{}/api/v0/collections/Tally", cluster.api_url(0)))
            .json(&serde_json::json!({"name": "first", "hits": -1}))
            .send()
            .await
            .expect("POST document");
        assert!(!response.status().is_success(), "{}", response.status());
        let body = response.text().await.expect("error body");
        assert!(body.contains(REFUSAL), "{body}");
        assert_no_documents(&cluster);
    })
    .await;
}

#[nuclei::test]
async fn cli_create_rejects_a_negative_pcounter() {
    edge::compat(async move {
        let cluster = single_node().await;
        let error = cluster
            .client(0)
            .collection_create("Tally", r#"{"name": "first", "hits": -1}"#)
            .expect_err("negative create");
        assert!(error.to_string().contains(REFUSAL), "{error}");
        assert_no_documents(&cluster);
    })
    .await;
}

#[nuclei::test]
async fn graphql_batch_create_rejects_a_negative_pcounter() {
    edge::compat(async move {
    let cluster = single_node().await;
    let error = cluster
        .client(0)
        .query_expect_error(
            r#"mutation { add_Tally(input: [{name: "a", hits: 1}, {name: "b", hits: -1}]) { _docID } }"#,
        )
        .expect("negative batch create error");
    assert!(error.contains(REFUSAL), "{error}");
    assert_no_documents(&cluster);
})
    .await;
}
