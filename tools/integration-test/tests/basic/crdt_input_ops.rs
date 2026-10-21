//! CRDT ops under an update's input, over GraphQL, HTTP and the CLI.

use integration_test::{DefraClient, TestCluster};
use serde_json::{json, Value};

const SCHEMA: &str = r#"
type Note {
  title: String
  body: String @crdt(type: text)
  views: Int @crdt(type: pcounter)
  set: String
}
"#;

const FIELDS: &str = "title body views set";

async fn note(cluster: &TestCluster) -> (DefraClient, String) {
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();
    let created = client
        .query(
            r#"mutation { add_Note(input: {title: "a", body: "hello world", views: 1, set: "s"}) { _docID } }"#,
        )
        .unwrap();
    let doc_id = created["add_Note"][0]["_docID"]
        .as_str()
        .expect("created Note has a _docID")
        .to_string();
    (client, doc_id)
}

fn update(client: &DefraClient, doc_id: &str, input: &str) -> Value {
    let result = client
        .query(&format!(
            r#"mutation {{ update_Note(docID: "{doc_id}", input: {input}) {{ {FIELDS} }} }}"#
        ))
        .unwrap_or_else(|error| panic!("update with {input} failed: {error}"));
    result["update_Note"][0].clone()
}

fn stored(client: &DefraClient) -> Value {
    client
        .query(&format!("query {{ Note {{ {FIELDS} }} }}"))
        .unwrap()["Note"][0]
        .clone()
}

#[tokio::test]
async fn graphql_update_applies_each_op_and_keeps_field_keyed_input() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let (client, doc_id) = note(&cluster).await;

    let after = update(
        &client,
        &doc_id,
        r#"{insert: {body: {at: 6, value: "brave "}}, increment: {views: 2}, set: {set: "t"}}"#,
    );
    assert_eq!(
        after,
        json!({"title": "a", "body": "hello brave world", "views": 3, "set": "t"})
    );

    let after = update(&client, &doc_id, r#"{title: "b", views: 1}"#);
    assert_eq!(after["title"], "b");
    assert_eq!(after["views"], 4, "a field-keyed counter still increments");

    let after = update(&client, &doc_id, r#"{set: {views: 10}}"#);
    assert_eq!(
        after["views"], 10,
        "an explicit set moves the counter to it"
    );

    let after = update(&client, &doc_id, r#"{delete: {body: {at: 0, length: 6}}}"#);
    assert_eq!(after["body"], "brave world");
    let after = update(
        &client,
        &doc_id,
        r#"{replace: {body: {at: 0, length: 5, value: "bold"}}}"#,
    );
    assert_eq!(after["body"], "bold world");

    assert_eq!(
        stored(&client),
        json!({"title": "b", "body": "bold world", "views": 10, "set": "t"})
    );
}

#[tokio::test]
async fn graphql_update_explains_an_op_on_the_wrong_field() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let (client, doc_id) = note(&cluster).await;

    for (input, expected) in [
        (r#"{increment: {title: 1}}"#, "not one"),
        (r#"{insert: {title: {at: 0, value: "x"}}}"#, "not one"),
        (r#"{set: {views: 0}}"#, "only grows"),
        (r#"{set: "x"}"#, "set: { set: ... }"),
        (r#"{title: "x", set: {title: "y"}}"#, "more than one op"),
    ] {
        let error = client
            .query(&format!(
                r#"mutation {{ update_Note(docID: "{doc_id}", input: {input}) {{ _docID }} }}"#
            ))
            .expect_err(input)
            .to_string();
        assert!(error.contains(expected), "{input}: {error}");
    }
    assert_eq!(
        stored(&client),
        json!({"title": "a", "body": "hello world", "views": 1, "set": "s"})
    );
}

#[tokio::test]
async fn http_update_takes_ops_in_its_patch() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let (client, doc_id) = note(&cluster).await;

    let response = reqwest::Client::new()
        .patch(format!(
            "{}/api/v0/collections/Note/document/{doc_id}",
            cluster.api_url(0)
        ))
        .json(&json!({"insert": {"body": {"at": 11, "value": "!"}}, "increment": {"views": 4}}))
        .send()
        .await
        .expect("update request");
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    assert!(status.is_success(), "update failed: {status} {body}");

    let response = reqwest::Client::new()
        .patch(format!("{}/api/v0/collections/Note", cluster.api_url(0)))
        .json(&json!({
            "filter": r#"{title: {_eq: "a"}}"#,
            "updater": r#"{"set": {"title": "c"}}"#,
        }))
        .send()
        .await
        .expect("filtered update request");
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    assert!(
        status.is_success(),
        "filtered update failed: {status} {body}"
    );

    assert_eq!(
        stored(&client),
        json!({"title": "c", "body": "hello world!", "views": 5, "set": "s"})
    );
}

#[tokio::test]
async fn cli_update_takes_ops_in_its_updater() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let (client, doc_id) = note(&cluster).await;

    client
        .collection_update(
            "Note",
            &doc_id,
            r#"{"insert": {"body": {"at": 0, "value": "> "}}, "set": {"set": "u"}}"#,
        )
        .expect("CLI update");

    assert_eq!(
        stored(&client),
        json!({"title": "a", "body": "> hello world", "views": 1, "set": "u"})
    );
}
