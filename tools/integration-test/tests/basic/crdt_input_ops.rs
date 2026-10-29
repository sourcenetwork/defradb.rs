//! CRDT ops under an update's input, over GraphQL, HTTP and the CLI.

use integration_test::{DefraClient, TestCluster};
use serde_json::{json, Value};

const SCHEMA: &str = r#"
type Note {
  title: String
  body: String @crdt(type: text)
  views: Int @crdt(type: pcounter)
}
"#;

const FIELDS: &str = "title body views";

async fn note(cluster: &TestCluster) -> (DefraClient, String) {
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();
    let created = client
        .query(
            r#"mutation { add_Note(input: {title: "a", body: "hello world", views: 1}) { _docID } }"#,
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

#[nuclei::test]
async fn graphql_update_applies_each_op_and_keeps_field_keyed_input() {
    edge::compat(async move {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let (client, doc_id) = note(&cluster).await;

    let after = update(
        &client,
        &doc_id,
        r#"{insert: {body: {at: 6, value: "brave "}}, increment: {views: 2}, set: {title: "t"}}"#,
    );
    assert_eq!(
        after,
        json!({"title": "t", "body": "hello brave world", "views": 3})
    );

    let after = update(&client, &doc_id, r#"{title: "b", views: 1}"#);
    assert_eq!(after["title"], "b");
    assert_eq!(after["views"], 4, "a field-keyed counter still increments");

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
        json!({"title": "b", "body": "bold world", "views": 4})
    );
})
    .await;
}

#[nuclei::test]
async fn graphql_update_explains_an_op_on_the_wrong_field() {
    edge::compat(async move {
        let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
        let (client, doc_id) = note(&cluster).await;

        for (input, expected) in [
            (r#"{increment: {title: 1}}"#, "do not support increment"),
            (
                r#"{insert: {title: {at: 0, value: "x"}}}"#,
                "do not support edit",
            ),
            (r#"{set: {views: 0}}"#, "use `increment`"),
            (r#"{increment: {views: -1}}"#, "cannot be negative"),
            (r#"{set: "x"}"#, "keyed by field name"),
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
            json!({"title": "a", "body": "hello world", "views": 1})
        );
    })
    .await;
}

#[nuclei::test]
async fn http_update_takes_ops_in_its_patch() {
    edge::compat(async move {
        let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
        let (client, doc_id) = note(&cluster).await;

        let response = net::http::Client::new(edge::testing::edge())
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

        let response = net::http::Client::new(edge::testing::edge())
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
            json!({"title": "c", "body": "hello world!", "views": 5})
        );
    })
    .await;
}

#[nuclei::test]
async fn cli_update_takes_ops_in_its_updater() {
    edge::compat(async move {
        let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
        let (client, doc_id) = note(&cluster).await;

        client
            .collection_update(
                "Note",
                &doc_id,
                r#"{"insert": {"body": {"at": 0, "value": "> "}}, "set": {"title": "u"}}"#,
            )
            .expect("CLI update");

        assert_eq!(
            stored(&client),
            json!({"title": "u", "body": "> hello world", "views": 1})
        );
    })
    .await;
}

#[nuclei::test]
async fn a_field_named_like_an_op_is_that_field() {
    edge::compat(async move {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client
        .schema_add("type Flag { delete: Boolean } type Doc { set: JSON, name: String }")
        .unwrap();

    let flag = client
        .query(r#"mutation { add_Flag(input: {delete: false}) { _docID } }"#)
        .unwrap()["add_Flag"][0]["_docID"]
        .as_str()
        .expect("created Flag has a _docID")
        .to_string();
    let updated = client
        .query(&format!(
            r#"mutation {{ update_Flag(docID: "{flag}", input: {{delete: true}}) {{ delete }} }}"#
        ))
        .unwrap();
    assert_eq!(updated["update_Flag"][0]["delete"], true);

    let doc = client
        .query(r#"mutation { add_Doc(input: {name: "a"}) { _docID } }"#)
        .unwrap()["add_Doc"][0]["_docID"]
        .as_str()
        .expect("created Doc has a _docID")
        .to_string();
    let updated = client
        .query(&format!(
            r#"mutation {{ update_Doc(docID: "{doc}", input: {{set: {{name: "x"}}}}) {{ set name }} }}"#
        ))
        .unwrap();
    assert_eq!(
        updated["update_Doc"][0],
        json!({"set": {"name": "x"}, "name": "a"})
    );
})
    .await;
}

#[nuclei::test]
async fn graphql_update_refuses_a_relation_and_its_id_together() {
    edge::compat(async move {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client
        .schema_add(
            "type Author { name: String books: [Book] }
             type Book { title: String author: Author }",
        )
        .unwrap();
    let book = client
        .query(r#"mutation { add_Book(input: {title: "t"}) { _docID } }"#)
        .unwrap()["add_Book"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();

    let error = client
        .query(&format!(
            r#"mutation {{ update_Book(docID: "{book}", input: {{_authorID: "a", set: {{author: "b"}}}}) {{ _docID }} }}"#
        ))
        .expect_err("relation and its ID in one update")
        .to_string();
    assert!(error.contains("more than one op"), "{error}");
})
    .await;
}
