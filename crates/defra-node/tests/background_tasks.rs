//! An `Node` runs the database's periodic tasks itself, as the CLI
//! and the `embedded` node do (#1991).

use std::time::{Duration, Instant};

use defra_node::Node;

const ROLLUP_SDL: &str = r#"
type MetricRollup @downsample(interval: "1000ms", timeField: "ts") {
  label: String
  source_doc_id: String
  source_height: Int
  window_start: DateTime
  window_end: DateTime
  count: Int
  sum: Int
  avg: Float
  min: Int
  max: Int
}
"#;

#[test]
fn downsample_view_refreshes_without_a_manual_refresh() {
    workers::block_on(async {
    let node = Node::builder().build().await.unwrap();
    node.add_collection("type Metric { label: String  ts: DateTime  value: Int }")
        .await
        .unwrap();
    node.add_view("Metric { label ts value }", ROLLUP_SDL)
        .await
        .unwrap();

    // A long-closed window.
    let response = node
        .execute(
            r#"mutation { add_Metric(input: {label: "cpu", ts: "2020-01-01T00:00:00Z", value: 7}) { _docID } }"#,
        )
        .await;
    assert!(!response.has_errors(), "{:?}", response.errors);

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let response = node.execute("{ MetricRollup { count sum } }").await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let rows = response.data.unwrap()["MetricRollup"].clone();
        if rows.as_array().is_some_and(|rows| !rows.is_empty()) {
            assert_eq!(rows[0]["sum"], 7);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "MetricRollup never refreshed: {rows}"
        );
        workers::delay(Duration::from_millis(100)).unwrap().await.unwrap();
    }

    node.shutdown().await;
})
    .unwrap()
}
