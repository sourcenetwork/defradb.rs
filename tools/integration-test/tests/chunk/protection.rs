//! Users cannot write chunks or the chunk list.

use integration_test::TestCluster;

use super::support::{create, CHUNKS, SCHEMA};

#[tokio::test]
async fn the_collection_api_cannot_write_chunks_or_the_chunk_list() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();
    let doc_id = create(&client, "Some text.");

    let err = client
        .collection_create(CHUNKS, r#"{"text": "forged", "hash": "00"}"#)
        .unwrap_err()
        .to_string();
    assert!(err.contains("written only by writes to Article"), "{err}");

    let err = client
        .collection_create(
            "Article",
            r#"{"title": "t", "body": "x", "_body_chunks": ["aa", "bb"]}"#,
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("read-only"), "{err}");
    let err = client
        .collection_update("Article", &doc_id, r#"{"_body_chunks": ["aa", "bb"]}"#)
        .unwrap_err()
        .to_string();
    assert!(err.contains("read-only"), "{err}");

    let data = client
        .query(&format!(
            "query {{ {CHUNKS} {{ text }} Article {{ _body_chunks }} }}"
        ))
        .unwrap();
    assert_eq!(data[CHUNKS], serde_json::json!([]));
    assert_eq!(
        data["Article"],
        serde_json::json!([{ "_body_chunks": null }])
    );
}
