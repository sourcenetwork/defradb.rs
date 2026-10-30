//! Users cannot write chunks, the chunk list, or chunk settings.

use integration_test::TestCluster;

use super::support::{body, create, visible, CHUNKS, SCHEMA};

#[tokio::test]
async fn chunks_and_their_settings_are_read_only() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();
    let doc_id = create(&client, &body(3, "rivers"));
    let before = visible(&client, None);
    let chunk_id = before[0].0.clone();

    let err = client
        .query_expect_error(&format!(
            r#"mutation {{ add_{CHUNKS}(input: {{text: "forged"}}) {{ _docID }} }}"#
        ))
        .unwrap();
    assert!(!err.is_empty());

    let err = client
        .collection_create(CHUNKS, r#"{"text": "forged", "hash": "00"}"#)
        .unwrap_err()
        .to_string();
    assert!(err.contains("written only by writes to Article"), "{err}");
    assert!(client
        .collection_update(CHUNKS, &chunk_id, r#"{"text": "forged"}"#)
        .is_err());
    assert!(client.collection_delete(CHUNKS, &chunk_id).is_err());
    assert!(client.collection_truncate(CHUNKS).is_err());

    let err = client
        .query_expect_error(&format!(
            r#"mutation {{ update_Article(docID: "{doc_id}", input: {{_body_chunks: ["forged"]}}) {{ _docID }} }}"#
        ))
        .unwrap();
    assert!(!err.is_empty());

    let err = client
        .collection_create(
            "Article",
            r#"{"body": "Some text.", "_body_chunks": ["forged"]}"#,
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("read-only"), "{err}");
    let err = client
        .collection_update("Article", &doc_id, r#"{"_body_chunks": ["forged"]}"#)
        .unwrap_err()
        .to_string();
    assert!(err.contains("read-only"), "{err}");

    let err = client
        .collection_patch(
            r#"[{"op":"replace","path":"/Article/ChunkedFields/0/MaxChars","value":100}]"#,
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("chunk settings cannot be mutated"), "{err}");

    assert_eq!(visible(&client, None), before);
}
