use defra_node::EmbeddedNode;
use serde_json::Value;

async fn data(node: &EmbeddedNode, query: &str) -> Value {
    let response = node.execute(query).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()
}

fn fetches(value: &Value) -> u64 {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| {
                if key == "docFetches" {
                    value.as_u64().unwrap_or(0)
                } else {
                    fetches(value)
                }
            })
            .sum(),
        Value::Array(items) => items.iter().map(fetches).sum(),
        _ => 0,
    }
}

#[tokio::test]
async fn exact_id_reads_seek_and_keep_filters_and_deleted_visibility() {
    let node = EmbeddedNode::builder().build().await.unwrap();
    node.add_schema("type Note { title: String }")
        .await
        .unwrap();
    let mut id = String::new();
    for n in 0..64 {
        let row = data(
            &node,
            &format!(r#"mutation {{ create_Note(input: {{title: "note-{n}"}}) {{ _docID }} }}"#),
        )
        .await;
        id = row["add_Note"][0]["_docID"]
            .as_str()
            .unwrap_or_else(|| panic!("mutation result: {row}"))
            .to_owned();
    }
    for selection in [
        format!(r#"filter: {{_docID: {{_eq: "{id}"}}}}"#),
        format!(r#"docID: "{id}""#),
    ] {
        let query = format!(r#"{{ Note({selection}) {{ _docID title }} }}"#);
        assert_eq!(data(&node, &query).await["Note"][0]["title"], "note-63");
        let explain = data(&node, &format!("query @explain(type: execute) {query}")).await;
        assert_eq!(fetches(&explain), 1, "{explain}");
    }
    let excluded = format!(
        r#"{{ Note(filter: {{_docID: {{_eq: "{id}"}}, title: {{_eq: "absent"}}}}) {{ _docID }} }}"#
    );
    assert_eq!(
        data(&node, &excluded).await["Note"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let missing = r#"{ Note(filter: {_docID: {_eq: "missing"}}) { _docID } }"#;
    let explain = data(&node, &format!("query @explain(type: execute) {missing}")).await;
    assert_eq!(fetches(&explain), 0, "{explain}");
    data(
        &node,
        &format!(r#"mutation {{ delete_Note(docID: "{id}") {{ _docID }} }}"#),
    )
    .await;
    assert_eq!(
        data(&node, &format!(r#"{{ Note(docID: "{id}") {{ _docID }} }}"#)).await["Note"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        data(
            &node,
            &format!(r#"{{ Note(docID: "{id}", showDeleted: true) {{ _docID }} }}"#)
        )
        .await["Note"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
