use integration_test::{DefraClient, TestCluster};
use query::explain_asserter::ExplainAsserter;
use serde_json::Value;

/// Mirrors the Go fixture from TestDebugExplainRequestWithOrderByRelationFieldWithIndex:
/// `Publisher` is the primary side, so it holds the `_bookID` foreign key, and the
/// ordering field lives on the child `Book`.
fn add_collection(node: &DefraClient) {
    node.schema_add(
        r#"
        type Book {
            title: String
            rating: Int @index
            publisher: Publisher
        }

        type Publisher {
            name: String
            book: Book @primary
        }
        "#,
    )
    .expect("add schema");
}

/// Names of the children of the `sequenceNode` under the `typeIndexJoin`, in order.
fn sequence_child_names(explain: &Value) -> Vec<String> {
    let sequence = ExplainAsserter::new(explain)
        .path(&[
            "explain",
            "operationNode",
            "0",
            "selectTopNode",
            "selectNode",
            "typeIndexJoin",
            "sequenceNode",
        ])
        .node()
        .as_array()
        .expect("sequenceNode array");

    sequence
        .iter()
        .map(|child| {
            child
                .as_object()
                .and_then(|obj| obj.keys().next())
                .unwrap_or_else(|| panic!("sequenceNode child is not a single-key object: {child}"))
                .clone()
        })
        .collect()
}

/// Assert the `typeJoinOne` child has Go's shape:
/// `{root: {scanNode}, subType: {selectTopNode: {selectNode: {scanNode}}}}`.
fn assert_join_one_shape(explain: &Value) {
    ExplainAsserter::new(explain)
        .path(&[
            "explain",
            "operationNode",
            "0",
            "selectTopNode",
            "selectNode",
            "typeIndexJoin",
            "sequenceNode",
            "typeJoinOne",
            "root",
            "scanNode",
        ])
        .node();

    ExplainAsserter::new(explain)
        .path(&[
            "explain",
            "operationNode",
            "0",
            "selectTopNode",
            "selectNode",
            "typeIndexJoin",
            "sequenceNode",
            "typeJoinOne",
            "subType",
            "selectTopNode",
            "selectNode",
            "scanNode",
        ])
        .node();
}

async fn debug_explain_asc_test(cluster: TestCluster) {
    let node = cluster.client(0);
    add_collection(&node);

    let explain = node
        .query(
            r#"query @explain(type: debug) @exhaustive {
                Publisher(order: {book: {rating: ASC}}) {
                    name
                }
            }"#,
        )
        .expect("debug explain ASC");

    assert_eq!(
        sequence_child_names(&explain),
        vec!["orphanNode", "typeJoinOne"],
        "ASC orders orphans first: {explain}"
    );
    assert_join_one_shape(&explain);
}

async fn debug_explain_desc_test(cluster: TestCluster) {
    let node = cluster.client(0);
    add_collection(&node);

    let explain = node
        .query(
            r#"query @explain(type: debug) @exhaustive {
                Publisher(order: {book: {rating: DESC}}) {
                    name
                }
            }"#,
        )
        .expect("debug explain DESC");

    assert_eq!(
        sequence_child_names(&explain),
        vec!["typeJoinOne", "orphanNode"],
        "DESC orders orphans last: {explain}"
    );
    assert_join_one_shape(&explain);
}

async fn exhaustive_execution_test(cluster: TestCluster) {
    let node = cluster.client(0);
    add_collection(&node);

    let add_book = |title: &str, rating: i64| {
        let result = node
            .query(&format!(
                r#"mutation {{ add_Book(input: {{title: "{title}", rating: {rating}}}) {{ _docID }} }}"#
            ))
            .expect("add book");
        result["add_Book"][0]["_docID"]
            .as_str()
            .expect("book _docID")
            .to_string()
    };

    let high = add_book("HighRated", 2);
    let low = add_book("LowRated", 1);

    let add_publisher = |name: &str, book: Option<&str>| {
        let input = match book {
            Some(book_id) => format!(r#"{{name: "{name}", book: "{book_id}"}}"#),
            None => format!(r#"{{name: "{name}"}}"#),
        };
        node.query(&format!(
            r#"mutation {{ add_Publisher(input: {input}) {{ _docID }} }}"#
        ))
        .expect("add publisher");
    };

    add_publisher("HighPublisher", Some(&high));
    add_publisher("LowPublisher", Some(&low));
    add_publisher("OrphanPublisher", None);

    let plain = node
        .query(
            r#"query {
                Publisher(order: {book: {rating: ASC}}) {
                    name
                }
            }"#,
        )
        .expect("query publishers ASC");
    assert_eq!(
        plain["Publisher"],
        serde_json::json!([
            {"name": "LowPublisher"},
            {"name": "HighPublisher"}
        ])
    );

    let exhaustive_asc = node
        .query(
            r#"query @exhaustive {
                Publisher(order: {book: {rating: ASC}}) {
                    name
                }
            }"#,
        )
        .expect("query publishers exhaustive ASC");
    assert_eq!(
        exhaustive_asc["Publisher"],
        serde_json::json!([
            {"name": "OrphanPublisher"},
            {"name": "LowPublisher"},
            {"name": "HighPublisher"}
        ])
    );

    let exhaustive_desc = node
        .query(
            r#"query @exhaustive {
                Publisher(order: {book: {rating: DESC}}) {
                    name
                }
            }"#,
        )
        .expect("query publishers exhaustive DESC");
    assert_eq!(
        exhaustive_desc["Publisher"],
        serde_json::json!([
            {"name": "HighPublisher"},
            {"name": "LowPublisher"},
            {"name": "OrphanPublisher"}
        ])
    );
}

#[nuclei::test]
async fn rust_relation_order_panic_1594_debug_explain_asc() {
    edge::compat(async {
        let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
        debug_explain_asc_test(cluster).await;
    })
    .await;
}

#[nuclei::test]
async fn rust_relation_order_panic_1594_debug_explain_desc() {
    edge::compat(async {
        let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
        debug_explain_desc_test(cluster).await;
    })
    .await;
}

#[nuclei::test]
async fn rust_relation_order_panic_1594_exhaustive_execution() {
    edge::compat(async {
        let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
        exhaustive_execution_test(cluster).await;
    })
    .await;
}
