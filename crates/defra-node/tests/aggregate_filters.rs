//! Top-level aggregate filters are validated like collection filters, and an
//! indexed filter counts the same documents as a full scan (#1995).

use defra_node::EmbeddedNode;
use serde_json::json;

async fn node() -> EmbeddedNode {
    let node = EmbeddedNode::builder().build().await.unwrap();
    node.add_schema("type Users { name: String @index  age: Int }")
        .await
        .unwrap();
    let response = node
        .execute(
            r#"mutation { add_Users(input: [
                {name: "ready", age: 1},
                {name: "ready", age: 2},
                {name: "done", age: 3}
            ]) { _docID } }"#,
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    node
}

#[tokio::test]
async fn unknown_aggregate_filter_field_is_an_error_not_zero() {
    let node = node().await;
    for query in [
        r#"{ COUNT(Users: {filter: {and: [{name: {_eq: "ready"}}]}}) }"#,
        r#"{ COUNT(Users: {filter: {nmae: {_eq: "ready"}}}) }"#,
    ] {
        let response = node.execute(query).await;
        assert!(
            response.has_errors(),
            "{query} returned {:?}",
            response.data
        );
        assert!(
            format!("{:?}", response.errors).contains("Unknown field"),
            "{:?}",
            response.errors
        );
    }
}

#[tokio::test]
async fn indexed_aggregate_filters_match_the_documents() {
    let node = node().await;
    for (query, expected) in [
        (
            r#"{ COUNT(Users: {filter: {name: {_eq: "ready"}}}) }"#,
            json!({"COUNT": 2}),
        ),
        (
            r#"{ COUNT(Users: {filter: {name: {_eq: "missing"}}}) }"#,
            json!({"COUNT": 0}),
        ),
        (
            r#"{ SUM(Users: {field: age, filter: {name: {_eq: "ready"}}}) }"#,
            json!({"SUM": 3}),
        ),
        (
            r#"{ COUNT(Users: {filter: {_and: [{name: {_eq: "ready"}}, {age: {_gt: 1}}]}}) }"#,
            json!({"COUNT": 1}),
        ),
    ] {
        let response = node.execute(query).await;
        assert!(!response.has_errors(), "{query}: {:?}", response.errors);
        assert_eq!(response.data.unwrap(), expected, "{query}");
    }
}
