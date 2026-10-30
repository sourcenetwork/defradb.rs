//! Concurrent updates and a delete of one parent leave exactly the chunks of
//! the text that won, or none for a deleted parent.

use integration_test::{DefraClient, TestCluster};

use super::support::{body, create, list_of, visible_hashes, CHUNKS, SCHEMA};

/// The parent's text, rebuilt from its chunks in list order. With no overlap,
/// the chunks join into the text exactly.
fn text_from_chunks(client: &DefraClient, doc_id: &str) -> String {
    let data = client
        .query(&format!("query {{ {CHUNKS} {{ hash text }} }}"))
        .unwrap();
    let texts: std::collections::BTreeMap<String, String> = data[CHUNKS]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["hash"].as_str().unwrap().to_string(),
                c["text"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    list_of(client, doc_id, None)
        .iter()
        .skip(1)
        .map(|hash| texts[hash].clone())
        .collect()
}

fn stored_body(client: &DefraClient, doc_id: &str) -> Option<String> {
    let data = client
        .query(&format!(
            r#"query {{ Article(docID: "{doc_id}") {{ body }} }}"#
        ))
        .unwrap();
    data["Article"][0]["body"].as_str().map(str::to_owned)
}

fn update_mutation(doc_id: &str, i: usize) -> String {
    format!(
        r#"mutation {{ update_Article(docID: "{doc_id}", input: {{body: "{}"}}) {{ _docID }} }}"#,
        body(3 + i % 4, &format!("topic {i}"))
    )
}

#[tokio::test]
async fn concurrent_updates_leave_the_chunks_of_the_winning_text() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();
    let doc_id = create(&client, &body(4, "start"));

    let succeeded = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let client = cluster.client(0);
                let mutation = update_mutation(&doc_id, i);
                scope.spawn(move || client.query(&mutation).is_ok())
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|ok| *ok)
            .count()
    });
    assert!(succeeded > 0, "every concurrent update failed");

    let stored = stored_body(&client, &doc_id).expect("parent still exists");
    assert_eq!(text_from_chunks(&client, &doc_id), stored);
    let mut listed: Vec<String> = list_of(&client, &doc_id, None)
        .into_iter()
        .skip(1)
        .collect();
    listed.sort();
    listed.dedup();
    assert_eq!(visible_hashes(&client, None), listed);
}

#[tokio::test]
async fn concurrent_updates_and_a_delete_leave_no_stray_chunk() {
    let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
    let client = cluster.client(0);
    client.schema_add(SCHEMA).unwrap();
    let doc_id = create(&client, &body(4, "start"));

    let succeeded = std::thread::scope(|scope| {
        let mut handles: Vec<_> = (0..6)
            .map(|i| {
                let client = cluster.client(0);
                let mutation = update_mutation(&doc_id, i);
                scope.spawn(move || client.query(&mutation).is_ok())
            })
            .collect();
        let client = cluster.client(0);
        let delete = format!(r#"mutation {{ delete_Article(docID: "{doc_id}") {{ _docID }} }}"#);
        handles.push(scope.spawn(move || client.query(&delete).is_ok()));
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|ok| *ok)
            .count()
    });
    assert!(succeeded > 0, "every concurrent write failed");

    match stored_body(&client, &doc_id) {
        None => assert!(visible_hashes(&client, None).is_empty()),
        Some(stored) => assert_eq!(text_from_chunks(&client, &doc_id), stored),
    }
}
