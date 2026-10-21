use defra_node::EmbeddedNode;
use schema::IndexedFieldDescription;

#[test]
fn reconcile_indexes_preserves_documents_and_uses_completed_composite() {
    workers::block_on(async {
    let node = EmbeddedNode::builder().build().await.unwrap();
    node.add_schema("type IndexedNote { node: String @index session: String @index text: String }")
        .await
        .unwrap();
    let response = node.execute(r#"mutation { create_IndexedNote(input: [{node: "node", session: "session", text: "first"}, {node: "node", session: "session", text: "second"}, {node: "other", session: "session", text: "foreign"}]) { _docID } }"#).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    let before = node.get_collection("IndexedNote").unwrap().unwrap();
    let query = "{ IndexedNote(order: {text: ASC}) { _docID node session text } }";
    let documents = node.execute(query).await.data.unwrap();
    let fields = ["node", "session"]
        .map(|name| IndexedFieldDescription {
            name: name.into(),
            descending: false,
        })
        .to_vec();
    let first = node
        .ensure_ordinary_index("IndexedNote", "node_session", fields.clone())
        .await
        .unwrap();
    let (left, right) = futures::join!(
        node.ensure_ordinary_index("IndexedNote", "node_session", fields.clone()),
        node.ensure_ordinary_index("IndexedNote", "node_session", fields),
    );
    assert_eq!(left.unwrap(), first);
    assert_eq!(right.unwrap(), first);
    let after = node.get_collection("IndexedNote").unwrap().unwrap();
    assert_eq!(before.version_id, after.version_id);
    assert_eq!(before.collection_id, after.collection_id);
    for index in &before.indexes {
        assert!(
            after
                .indexes
                .iter()
                .any(|current| serde_json::to_value(current).unwrap()
                    == serde_json::to_value(index).unwrap()),
            "existing index changed: {index:?}; after: {:?}",
            after.indexes
        );
    }
    assert_eq!(node.execute(query).await.data.unwrap(), documents);
    let id = documents["IndexedNote"][0]["_docID"].as_str().unwrap();
    let filtered = format!(
        r#"{{ IndexedNote(filter: {{node: {{_eq: "node"}}, session: {{_eq: "session"}}, _docID: {{_neq: "{id}"}}}}) {{ _docID text }} }}"#
    );
    let result = node.execute(&filtered).await;
    assert!(!result.has_errors(), "{:?}", result.errors);
    assert_eq!(
        result.data.unwrap()["IndexedNote"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let explain = node.execute(&format!("query @explain {filtered}")).await;
    assert!(!explain.has_errors(), "{:?}", explain.errors);
    assert!(explain.data.unwrap().to_string().contains("node_session"));
    node.shutdown().await;
})
    .unwrap()
}

fn scan_count(value: &serde_json::Value, name: &str) -> Option<u64> {
    match value {
        serde_json::Value::Object(fields) => fields
            .get(name)
            .and_then(|v| v.as_u64())
            .or_else(|| fields.values().find_map(|v| scan_count(v, name))),
        serde_json::Value::Array(values) => values.iter().find_map(|v| scan_count(v, name)),
        _ => None,
    }
}

#[test]
fn ordered_index_stream_stops_after_passing_residual_rows() {
    workers::block_on(async {
        let node = EmbeddedNode::builder().build().await.unwrap();
        node.add_schema("type Entry { scope: String seq: Int keep: Boolean }")
            .await
            .unwrap();
        node.ensure_ordinary_index(
            "Entry",
            "scope_seq",
            ["scope", "seq"]
                .map(|name| IndexedFieldDescription {
                    name: name.into(),
                    descending: false,
                })
                .to_vec(),
        )
        .await
        .unwrap();
        let inputs = (0..120)
            .map(|seq| format!("{{scope: \"s\", seq: {seq}, keep: {}}}", seq >= 20))
            .collect::<Vec<_>>()
            .join(",");
        let created = node
            .execute(&format!(
                "mutation {{ create_Entry(input: [{inputs}]) {{ _docID }} }}"
            ))
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let query = r#"query {
            Entry(filter: {scope: {_eq: "s"}, keep: {_eq: true}},
                  order: [{scope: ASC}, {seq: ASC}], limit: 4) { seq }
        }"#;
        let result = node.execute(query).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        assert_eq!(
            result.data.unwrap()["Entry"],
            serde_json::json!([
                {"seq":20}, {"seq":21}, {"seq":22}, {"seq":23}
            ])
        );
        let explained = node
            .execute(&query.replacen("query", "query @explain(type: execute)", 1))
            .await;
        assert!(!explained.has_errors(), "{:?}", explained.errors);
        let plan = explained.data.unwrap();
        let fetched = scan_count(&plan, "indexFetches").unwrap();
        assert!(
            (24..=25).contains(&fetched),
            "must consume rejected rows, then stop: {plan}"
        );
        node.shutdown().await;
    })
    .unwrap()
}
