//! Chunks replicate as ordinary documents, so a peer gets the same chunk
//! documents and doesn't split the text again.

use std::time::Duration;

use integration_test::{poll_until, setup_two_node_replicated};

use super::support::{body, create, listed, update, visible, SCHEMA};

#[tokio::test]
async fn a_peer_has_the_same_chunk_documents() {
    let (cluster, _addr) =
        setup_two_node_replicated(SCHEMA, &["Article", "Article__body_chunk"]).await;
    let (node0, node1) = (cluster.client(0), cluster.client(1));

    let doc_id = create(&node0, &body(5, "rivers"));
    let expected = visible(&node0, None);
    assert!(!expected.is_empty());
    poll_until(
        || visible(&node1, None) == expected,
        Duration::from_secs(30),
        Duration::from_millis(250),
        "chunks replicate with the same DocIDs",
    )
    .await;
    assert_eq!(listed(&node1, &doc_id, None), listed(&node0, &doc_id, None));

    update(&node0, &doc_id, &body(3, "mountains"));
    let expected = visible(&node0, None);
    poll_until(
        || visible(&node1, None) == expected,
        Duration::from_secs(30),
        Duration::from_millis(250),
        "the edit's chunks replicate and the superseded ones are hidden",
    )
    .await;
}

#[tokio::test]
async fn a_parent_delete_replicates_with_its_chunks() {
    let (cluster, _addr) =
        setup_two_node_replicated(SCHEMA, &["Article", "Article__body_chunk"]).await;
    let (node0, node1) = (cluster.client(0), cluster.client(1));

    let doc_id = create(&node0, &body(4, "lakes"));
    let expected = visible(&node0, None);
    poll_until(
        || visible(&node1, None) == expected,
        Duration::from_secs(30),
        Duration::from_millis(250),
        "chunks replicate",
    )
    .await;

    node0
        .query(&format!(
            r#"mutation {{ delete_Article(docID: "{doc_id}") {{ _docID }} }}"#
        ))
        .unwrap();
    poll_until(
        || {
            node1
                .query(&format!(
                    r#"query {{ Article(docID: "{doc_id}") {{ _docID }} }}"#
                ))
                .is_ok_and(|data| data["Article"].as_array().is_some_and(Vec::is_empty))
        },
        Duration::from_secs(30),
        Duration::from_millis(250),
        "the delete replicates",
    )
    .await;
    assert!(visible(&node1, None).is_empty());
}

#[tokio::test]
async fn a_replicator_of_the_parent_ships_its_chunks() {
    let (cluster, _addr) = setup_two_node_replicated(SCHEMA, &["Article"]).await;
    let (node0, node1) = (cluster.client(0), cluster.client(1));

    let doc_id = create(&node0, &body(4, "valleys"));
    let expected = visible(&node0, None);
    assert!(!expected.is_empty());
    poll_until(
        || visible(&node1, None) == expected,
        Duration::from_secs(30),
        Duration::from_millis(250),
        "a replicator naming only Article ships the chunks too",
    )
    .await;
    assert_eq!(listed(&node1, &doc_id, None), listed(&node0, &doc_id, None));
}
