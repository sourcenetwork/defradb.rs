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
