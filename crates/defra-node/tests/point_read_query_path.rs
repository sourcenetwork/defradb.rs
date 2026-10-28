use defra_node::Node;
use serde_json::Value;

async fn data(node: &Node, query: &str) -> Value {
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

#[test]
fn exact_id_reads_seek_and_keep_filters_and_deleted_visibility() {
    workers::block_on(async {
    let node = Node::builder().build().await.unwrap();
    node.add_collection("type Note { title: String owner: String @index }")
        .await
        .unwrap();
    let mut id = String::new();
    for n in 0..64 {
        let row = data(
            &node,
            &format!(r#"mutation {{ create_Note(input: {{title: "note-{n}", owner: "shared"}}) {{ _docID }} }}"#),
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
        format!(r#"filter: {{_docID: {{_eq: "{id}"}}, owner: {{_eq: "shared"}}}}"#),
        format!(r#"docID: "{id}", filter: {{owner: {{_eq: "shared"}}}}"#),
    ] {
        let query = format!(r#"{{ Note({selection}) {{ _docID title }} }}"#);
        assert_eq!(data(&node, &query).await["Note"][0]["title"], "note-63");
        let explain = data(&node, &format!("query @explain(type: execute) {query}")).await;
        assert_eq!(fetches(&explain), 1, "{explain}");
    }
    let excluded = format!(
        r#"{{ Note(filter: {{_docID: {{_eq: "{id}"}}, owner: {{_eq: "absent"}}}}) {{ _docID }} }}"#
    );
    assert_eq!(
        data(&node, &excluded).await["Note"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let missing =
        r#"{ Note(filter: {_docID: {_eq: "missing"}, owner: {_eq: "shared"}}) { _docID } }"#;
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
})
    .unwrap()
}

#[test]
fn deleted_rows_remain_visible_with_indexed_filters_and_ordering() {
    workers::block_on(async {
        let node = Node::builder().build().await.unwrap();
        node.add_collection(
            "type RetainedNote { owner: String @index rank: Int @index body: String }",
        )
        .await
        .unwrap();
        let created = data(
            &node,
            r#"mutation { create_RetainedNote(input: [
        {owner: "shared", rank: 1, body: "retained"},
        {owner: "shared", rank: 2, body: "live"},
        {owner: "other", rank: 3, body: "foreign"}
    ]) {_docID} }"#,
        )
        .await;
        let retired = created["add_RetainedNote"][0]["_docID"].as_str().unwrap();
        data(
            &node,
            &format!(r#"mutation {{delete_RetainedNote(docID: "{retired}") {{_docID}}}}"#),
        )
        .await;
        let expected = serde_json::json!([
            {"rank":1,"body":"retained","_deleted":true},
            {"rank":2,"body":"live","_deleted":false}
        ]);
        for selection in [
            r#"filter: {owner: {_eq: "shared"}}, order: {rank: ASC}"#.to_owned(),
            r#"filter: {rank: {_lte: 2}}, order: {rank: ASC}"#.to_owned(),
            format!(r#"docID: "{retired}", filter: {{owner: {{_eq: "shared"}}}}"#),
        ] {
            let query = format!(
                r#"{{RetainedNote({selection}, showDeleted: true) {{rank body _deleted}}}}"#
            );
            let rows = data(&node, &query).await;
            let selected = if selection.starts_with("docID") {
                serde_json::json!([expected[0]])
            } else {
                expected.clone()
            };
            assert_eq!(rows["RetainedNote"], selected, "{query}");
        }
        let all = data(
            &node,
            "{RetainedNote(showDeleted: true, order: {rank: ASC}) {rank}}",
        )
        .await;
        assert_eq!(
            all["RetainedNote"],
            serde_json::json!([{"rank":1},{"rank":2},{"rank":3}])
        );
        let first = data(
            &node,
            "{RetainedNote(showDeleted: true, limit: 1, order: {rank: ASC}) {rank}}",
        )
        .await;
        assert_eq!(first["RetainedNote"], serde_json::json!([{"rank":1}]));
        let second = data(
            &node,
            "{RetainedNote(showDeleted: true, limit: 1, offset: 1, order: {rank: ASC}) {rank}}",
        )
        .await;
        assert_eq!(second["RetainedNote"], serde_json::json!([{"rank":2}]));
        for selection in [
            r#"filter: {owner: {_eq: "absent"}}"#.to_owned(),
            format!(r#"docID: "{retired}", filter: {{owner: {{_eq: "other"}}}}"#),
        ] {
            let result = data(
                &node,
                &format!(r#"{{RetainedNote(showDeleted: true, {selection}) {{rank}}}}"#),
            )
            .await;
            assert_eq!(result["RetainedNote"], serde_json::json!([]));
        }
        let live = data(
            &node,
            r#"{RetainedNote(filter: {owner: {_eq: "shared"}}, order: {rank: ASC}) {rank}}"#,
        )
        .await;
        assert_eq!(live["RetainedNote"], serde_json::json!([{"rank":2}]));
        node.shutdown().await.unwrap();
    })
    .unwrap()
}
