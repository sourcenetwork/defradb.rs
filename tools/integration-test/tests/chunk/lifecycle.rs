//! Every write path keeps a parent's chunks exactly in step with its text.

use integration_test::TestCluster;

use super::support::{body, create, listed, update, visible_hashes, SCHEMA};

async fn node() -> TestCluster {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    cluster.client(0).schema_add(SCHEMA).unwrap();
    cluster
}

#[tokio::test]
async fn graphql_create_update_and_delete_keep_chunks_exact() {
    let cluster = node().await;
    let client = cluster.client(0);

    let doc_id = create(&client, &body(6, "rivers"));
    let created = listed(&client, &doc_id, None);
    assert!(created.len() > 1);
    assert_eq!(visible_hashes(&client, None), created);

    update(&client, &doc_id, &body(4, "mountains"));
    let updated = listed(&client, &doc_id, None);
    assert_ne!(updated, created);
    assert_eq!(visible_hashes(&client, None), updated);

    client
        .query(&format!(
            r#"mutation {{ delete_Article(docID: "{doc_id}") {{ _docID }} }}"#
        ))
        .unwrap();
    assert!(visible_hashes(&client, None).is_empty());
}

#[tokio::test]
async fn an_explicit_transaction_writes_chunks_on_commit() {
    let cluster = node().await;
    let client = cluster.client(0);

    let tx = client.tx_create().unwrap();
    let data = client
        .query_with_tx(
            &format!(
                r#"mutation {{ add_Article(input: {{title: "t", body: "{}"}}) {{ _docID }} }}"#,
                body(5, "forests")
            ),
            &tx,
        )
        .unwrap();
    let doc_id = data["add_Article"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(visible_hashes(&client, None).is_empty());
    client.tx_commit(&tx).unwrap();

    let list = listed(&client, &doc_id, None);
    assert!(!list.is_empty());
    assert_eq!(visible_hashes(&client, None), list);
}

#[tokio::test]
async fn document_api_writes_keep_chunks_exact() {
    let cluster = node().await;
    let client = cluster.client(0);

    let doc = format!(r#"{{"title": "t", "body": "{}"}}"#, body(5, "deserts"));
    let created = client.collection_create("Article", &doc).unwrap();
    let doc_id = created["_docID"]
        .as_str()
        .or_else(|| created[0]["_docID"].as_str())
        .expect("document add returns the docID")
        .to_string();
    assert_eq!(
        visible_hashes(&client, None),
        listed(&client, &doc_id, None)
    );

    client
        .collection_update(
            "Article",
            &doc_id,
            &format!(r#"{{"body": "{}"}}"#, body(3, "oceans")),
        )
        .unwrap();
    assert_eq!(
        visible_hashes(&client, None),
        listed(&client, &doc_id, None)
    );

    client.collection_delete("Article", &doc_id).unwrap();
    assert!(visible_hashes(&client, None).is_empty());
}

#[tokio::test]
async fn truncating_the_parent_empties_its_chunks() {
    let cluster = node().await;
    let client = cluster.client(0);
    create(&client, &body(4, "caves"));
    assert!(!visible_hashes(&client, None).is_empty());

    client.collection_truncate("Article").unwrap();

    assert!(visible_hashes(&client, None).is_empty());
}
