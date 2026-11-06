//! Two Rust peers edit one Text field while apart, then replicate: both end
//! at the same text, holding both edits.

use std::time::{Duration, Instant};

use integration_test::{DefraClient, TestCluster};

const SCHEMA: &str = "type Note { body: String @crdt(type: text) }";

fn address(info: serde_json::Value) -> String {
    info.as_array()
        .and_then(|addrs| addrs.first())
        .and_then(|addr| addr.as_str())
        .expect("node has a P2P address")
        .to_string()
}

#[nuclei::test]
async fn peers_editing_one_text_apart_converge_with_both_edits() {
    edge::compat(async {
        let cluster = TestCluster::builder()
            .rust_nodes(2)
            .with_p2p()
            .build()
            .await
            .unwrap();
        let alice = cluster.client(0);
        let bob = cluster.client(1);
        for node in [0, 1] {
            cluster
                .wait_for_log(node, "p2p_listening", Duration::from_secs(15))
                .await
                .expect("P2P listener started");
        }
        let alice_addr = address(alice.p2p_info().unwrap());
        let bob_addr = address(bob.p2p_info().unwrap());
        alice.schema_add(SCHEMA).unwrap();
        bob.schema_add(SCHEMA).unwrap();
        alice.p2p_connect(&[bob_addr.as_str()]).unwrap();

        let created = alice
            .query(r#"mutation { add_Note(input: {body: "hello world"}) { _docID } }"#)
            .unwrap();
        let doc_id = created["add_Note"][0]["_docID"]
            .as_str()
            .expect("created Note has a _docID")
            .to_string();
        alice.p2p_replicator_set(&["Note"], &bob_addr).unwrap();
        wait_for(&bob, |body| body == "hello world").await;
        alice
            .p2p_replicator_delete(&["Note"], Some(bob_addr.as_str()))
            .expect("stop replicating while the peers edit apart");

        alice
            .query(&format!(
                r#"mutation {{ update_Note(docID: "{doc_id}", input: {{body: "hello brave world"}}) {{ body }} }}"#
            ))
            .unwrap();
        bob.query(&format!(
            r#"mutation {{ update_Note(docID: "{doc_id}", input: {{body: "hello world!"}}) {{ body }} }}"#
        ))
        .unwrap();

        alice.p2p_replicator_set(&["Note"], &bob_addr).unwrap();
        bob.p2p_replicator_set(&["Note"], &alice_addr).unwrap();
        for node in [&alice, &bob] {
            wait_for(node, |body| body == "hello brave world!").await;
        }
    })
    .await;
}

/// An upsert that finds its document edits the text like an update does.
#[nuclei::test]
async fn an_upsert_edits_an_existing_text() {
    edge::compat(async {
        let cluster = TestCluster::builder().rust_nodes(1).build().await.unwrap();
        let node = cluster.client(0);
        node.schema_add("type Note { title: String body: String @crdt(type: text) }")
            .unwrap();
        let upsert = |body: &str| {
            node.query(&format!(
                r#"mutation {{ upsert_Note(filter: {{title: {{_eq: "a"}}}}, add: {{title: "a", body: "hello"}}, update: {{body: "{body}"}}) {{ _docID }} }}"#
            ))
            .unwrap()
        };
        upsert("unused");
        wait_for(&node, |body| body == "hello").await;
        upsert("hello world");
        wait_for(&node, |body| body == "hello world").await;
        upsert("help world!");
        wait_for(&node, |body| body == "help world!").await;
    })
    .await;
}

/// Poll `node` until its one Note's body passes `done`.
async fn wait_for<F>(node: &DefraClient, done: F)
where
    F: Fn(&str) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last = String::new();
    while Instant::now() < deadline {
        let result = node.query("query { Note { body } }").unwrap();
        if let Some(body) = result["Note"][0]["body"].as_str() {
            if done(body) {
                return;
            }
            last = body.to_string();
        }
        nuclei::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("text did not converge in time; last body {last:?}");
}
