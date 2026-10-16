//! `_commits` over a document whose `@immutable` fields share one block.

use integration_test::TestCluster;
use serde_json::Value;

const SCHEMA: &str =
    "type Record { author: String @immutable  kind: String @immutable  body: String }";

fn field_names(data: &Value) -> Vec<String> {
    let mut names: Vec<String> = data["_commits"]
        .as_array()
        .expect("_commits array")
        .iter()
        .map(|commit| commit["fieldName"].as_str().expect("fieldName").to_string())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn rust_commits_by_cid_list_every_immutable_field() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let node = cluster.client(0);
    node.schema_add(SCHEMA).expect("add schema");

    let create = node
        .query(r#"mutation { add_Record(input: {author: "alice", kind: "note", body: "hi"}) { _docID } }"#)
        .expect("create record");
    let doc_id = create["add_Record"][0]["_docID"]
        .as_str()
        .expect("_docID")
        .to_string();

    let author = node
        .query(&format!(
            r#"query {{ _commits(docID: "{doc_id}", filter: {{fieldName: {{_eq: "author"}}}}) {{ cid }} }}"#
        ))
        .expect("author commits");
    let immutable_cid = author["_commits"][0]["cid"]
        .as_str()
        .expect("immutable block cid")
        .to_string();

    let by_cid = node
        .query(&format!(
            r#"query {{ _commits(cid: "{immutable_cid}") {{ fieldName }} }}"#
        ))
        .expect("commits by cid");
    assert_eq!(field_names(&by_cid), ["author", "kind"]);

    let filtered = node
        .query(&format!(
            r#"query {{ _commits(cid: "{immutable_cid}", filter: {{fieldName: {{_eq: "kind"}}}}) {{ fieldName }} }}"#
        ))
        .expect("filtered commits by cid");
    assert_eq!(field_names(&filtered), ["kind"]);
}
