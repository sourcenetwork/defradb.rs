use defra_node::{EmbeddedNode, QueryRequest};
use serde_json::json;

#[tokio::test]
async fn update_and_upsert_resolve_object_variables() {
    let node = EmbeddedNode::builder().build().await.unwrap();
    node.add_schema("type Probe { key: String @index(unique: true) label: String payload: JSON }")
        .await
        .unwrap();
    let created = node
        .execute("mutation { add_Probe(input: {key: \"first\", label: \"before\"}) { _docID } }")
        .await;
    assert!(!created.has_errors(), "{:?}", created.errors);

    let updated = node.execute_request_with_retry(
        QueryRequest::new("mutation($input: ProbeMutationInputArg!) { update_Probe(filter: {key: {_eq: \"first\"}}, input: $input) { label payload } }")
            .with_variables(json!({"input": {"label": "after", "payload": {"nested": [1, null, true]}}})),
        Default::default(),
    ).await;
    assert!(!updated.has_errors(), "{:?}", updated.errors);
    assert_eq!(
        updated.data.as_ref().unwrap()["update_Probe"][0]["label"],
        "after"
    );

    for expected in ["created", "updated"] {
        let result = node.execute_request_with_retry(
            QueryRequest::new("mutation($add: ProbeMutationInputArg!, $update: ProbeMutationInputArg!) { upsert_Probe(filter: {key: {_eq: \"second\"}}, add: $add, update: $update) { label } }")
                .with_variables(json!({"add": {"key": "second", "label": "created"}, "update": {"label": "updated"}})),
            Default::default(),
        ).await;
        assert!(!result.has_errors(), "{:?}", result.errors);
        assert_eq!(
            result.data.as_ref().unwrap()["upsert_Probe"][0]["label"],
            expected
        );
    }
    let stored = node
        .execute("{ Probe(filter: {key: {_eq: \"first\"}}) { label payload } }")
        .await;
    assert!(!stored.has_errors(), "{:?}", stored.errors);
    assert_eq!(
        stored.data.unwrap()["Probe"][0],
        json!({"label": "after", "payload": {"nested": [1, null, true]}})
    );
    node.shutdown().await;
}
