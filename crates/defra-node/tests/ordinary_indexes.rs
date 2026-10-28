use defra_node::Node;
use schema::IndexedFieldDescription;

#[test]
fn reconcile_indexes_preserves_documents_and_uses_completed_composite() {
    workers::block_on(async {
    let node = Node::builder().build().await.unwrap();
    node.add_collection("type IndexedNote { node: String @index session: String @index text: String }")
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
    node.shutdown().await.unwrap();
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
        let node = Node::builder().build().await.unwrap();
        node.add_collection("type Entry { scope: String seq: Int keep: Boolean }")
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
        node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn ordered_prefix_sorts_only_the_boundary_tie_group() {
    workers::block_on(async {
    let node = Node::builder().build().await.unwrap();
    node.add_collection("type TieEntry @index(fields: [\"scope\", \"seq\"], unique: true) { scope: String seq: Int rank: Int }")
        .await.unwrap();
    let inputs = (0..120)
        .map(|seq| format!("{{scope: \"s\", seq: {seq}, rank: {}}}", 120 - seq))
        .chain([
            "{scope: \"s\", seq: null, rank: 2}".into(),
            "{scope: \"s\", seq: null, rank: 1}".into(),
        ])
        .collect::<Vec<_>>()
        .join(",");
    let created = node
        .execute(&format!(
            "mutation {{ create_TieEntry(input: [{inputs}]) {{ _docID }} }}"
        ))
        .await;
    assert!(!created.has_errors(), "{:?}", created.errors);
    for (direction, expected) in [
        ("DESC", vec![Some(119), Some(118), Some(117), Some(116)]),
        ("ASC", vec![None, None, Some(0), Some(1)]),
    ] {
        let query = format!("query {{ TieEntry(filter: {{scope: {{_eq: \"s\"}}}}, order: [{{seq: {direction}}}, {{_docID: ASC}}], limit: 4) {{ _docID seq rank }} }}");
        let response = node.execute(&query).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let data = response.data.unwrap();
        let rows = data["TieEntry"].as_array().unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row["seq"].as_i64())
                .collect::<Vec<_>>(),
            expected
        );
        if direction == "ASC" {
            assert!(rows[0]["_docID"].as_str() < rows[1]["_docID"].as_str());
        }
        let explained = node
            .execute(&query.replacen("query", "query @explain(type: execute)", 1))
            .await;
        assert!(!explained.has_errors(), "{:?}", explained.errors);
        let plan = explained.data.unwrap();
        let fetched = scan_count(&plan, "docFetches").unwrap();
        assert!(fetched <= 6, "page must not materialize 122 rows: {plan}");
    }
    node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn partial_index_order_keeps_residual_filter_and_page_offset() {
    workers::block_on(async {
    let node = Node::builder().build().await.unwrap();
    node.add_collection("type TiePage @index(fields: [\"scope\", \"seq\"]) { scope: String @index seq: Int rank: Int keep: Boolean }").await.unwrap();
    let input = (0..120)
        .map(|i| {
            format!(
                "{{scope: \"s\", seq: {}, rank: {}, keep: {}}}",
                i / 3,
                i % 3,
                i % 3 != 1
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let created = node
        .execute(&format!(
            "mutation {{ create_TiePage(input:[{input}]) {{_docID}} }}"
        ))
        .await;
    assert!(!created.has_errors(), "{:?}", created.errors);
    let query = r#"query { TiePage(filter:{scope:{_eq:"s"},keep:{_eq:true}},order:[{seq:DESC},{rank:ASC}],offset:1,limit:3){seq rank} }"#;
    let result = node.execute(query).await;
    assert!(!result.has_errors(), "{:?}", result.errors);
    assert_eq!(
        result.data.unwrap()["TiePage"],
        serde_json::json!([{"seq":39,"rank":2},{"seq":38,"rank":0},{"seq":38,"rank":2}])
    );
    let result = node
        .execute(&query.replacen("query", "query @explain(type:execute)", 1))
        .await;
    assert!(!result.has_errors(), "{:?}", result.errors);
    let plan = result.data.unwrap();
    assert!(
        scan_count(&plan, "docFetches").unwrap() <= 8,
        "must finish two matching tie groups: {plan}"
    );
    node.shutdown().await.unwrap();
    })
    .unwrap()
}
