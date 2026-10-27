//! Changing a text and changing it back shows the original chunk documents again.

use integration_test::TestCluster;

use super::support::{body, create, update, visible, SCHEMA};

#[tokio::test]
async fn reverting_the_text_reuses_the_original_chunks() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();

    let a = body(5, "rivers");
    let doc_id = create(&client, &a);
    let original = visible(&client, None);

    update(&client, &doc_id, &body(3, "mountains"));
    assert_ne!(visible(&client, None), original);
    update(&client, &doc_id, &a);

    assert_eq!(visible(&client, None), original);
}
