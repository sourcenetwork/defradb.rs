//! Issue #1194 regression: concurrent `update_X` mutations against DISTINCT
//! documents must not fail with `transaction conflict. Please retry`.
//!
//! Root cause: field-delta blocks are content-addressed (`b` + CID) and carry
//! no document identity. Two documents whose updates produce byte-identical
//! deltas (e.g. both rewrite `status: "streaming"` at the same chain depth)
//! write the SAME blockstore key, and the conflict tracker treated that blind
//! write-write overlap as a transaction conflict. Identical-content block
//! writes are idempotent and must not conflict; true same-document conflicts
//! must survive (see the control below).

use std::sync::Arc;

use integration_test::TestCluster;
use serde_json::{json, Value};

async fn build_cluster() -> TestCluster {
    TestCluster::builder()
        .rust_nodes(1)
        .with_store("regolith")
        .build()
        .await
        .expect("build cluster")
}

async fn gql(http: &net::http::Client, url: &str, query: &str) -> Result<Value, String> {
    let resp = http
        .post(format!("{url}/api/v0/graphql"))
        .json(&json!({ "query": query }))
        .send()
        .await
        .map_err(|e| format!("http error: {e}"))?;
    let body: Value = resp.json().await.map_err(|e| format!("bad json: {e}"))?;
    if let Some(errors) = body.get("errors").and_then(|e| e.as_array()) {
        if !errors.is_empty() {
            return Err(errors
                .iter()
                .map(|e| e["message"].as_str().unwrap_or("?").to_string())
                .collect::<Vec<_>>()
                .join("; "));
        }
    }
    Ok(body["data"].clone())
}

/// The reported workload over HTTP GraphQL on regolith: eight documents, eight
/// barrier-synchronized writers, each round rewriting `status: "streaming"`
/// unchanged (the snapshot-flush shape that makes delta blocks byte-identical
/// across documents). Every attempt must succeed on the first try — no client
/// retry budget.
#[nuclei::test(threads = 8)]
async fn issue1194_concurrent_distinct_doc_updates() {
    edge::compat(async {
        let cluster = build_cluster().await;
        let node = cluster.client(0);
        let api_url = cluster.api_url(0).to_string();

        node.schema_add("type AgentResponse { status: String  content: String }")
            .expect("schema add");

        let http = net::http::Client::new(edge::testing::edge());
        let mut doc_ids = Vec::new();
        for i in 0..8 {
            let data = gql(
                &http,
                &api_url,
                &format!(
                    r#"mutation {{ add_AgentResponse(input: {{status: "streaming", content: "init-{i}"}}) {{ _docID }} }}"#
                ),
            )
            .await
            .expect("create doc");
            doc_ids.push(
                data["add_AgentResponse"][0]["_docID"]
                    .as_str()
                    .expect("docID")
                    .to_string(),
            );
        }

        let rounds = 10;
        let barrier = Arc::new(regolith::sync::Barrier::new(doc_ids.len()));
        let mut handles = Vec::new();
        for (worker, doc_id) in doc_ids.iter().enumerate() {
            let http = http.clone();
            let url = api_url.to_string();
            let doc_id = doc_id.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(nuclei::spawn(async move {
                let mut failures = Vec::new();
                for round in 0..rounds {
                    barrier.wait().await;
                    let mutation = format!(
                        r#"mutation {{ update_AgentResponse(filter: {{_docID: {{_eq: "{doc_id}"}}, status: {{_eq: "streaming"}}}}, input: {{content: "w{worker}-r{round}", status: "streaming"}}) {{ _docID }} }}"#
                    );
                    match gql(&http, &url, &mutation).await {
                        Ok(data) => {
                            let n = data["update_AgentResponse"]
                                .as_array()
                                .map(|a| a.len())
                                .unwrap_or(0);
                            if n != 1 {
                                failures.push(format!("round {round}: updated {n} docs"));
                            }
                        }
                        Err(msg) => failures.push(format!("round {round}: {msg}")),
                    }
                }
                failures
            }));
        }

        for (worker, handle) in handles.into_iter().enumerate() {
            let failures = handle.join().await.expect("worker panicked");
            assert!(
                failures.is_empty(),
                "worker {worker} updating its own document failed: {failures:?}"
            );
        }

        // No writes lost, duplicated, or applied to the wrong document.
        for (worker, doc_id) in doc_ids.iter().enumerate() {
            let data = gql(
                &http,
                &api_url,
                &format!(r#"query {{ AgentResponse(docID: "{doc_id}") {{ content status }} }}"#),
            )
            .await
            .expect("final read");
            let last = rounds - 1;
            assert_eq!(
                data["AgentResponse"][0]["content"],
                json!(format!("w{worker}-r{last}")),
                "doc {worker} final content"
            );
            assert_eq!(data["AgentResponse"][0]["status"], json!("streaming"));
        }
    })
    .await;
}

/// Deterministic core of #1194: two interactive transactions with overlapping
/// snapshots update two DIFFERENT documents with a byte-identical field delta
/// (same `status` value at the same CRDT chain depth => same block CID). Both
/// commits must succeed. Before the fix the second commit always failed with
/// a write-write conflict on the shared block key.
#[nuclei::test(threads = 4)]
async fn issue1194_identical_delta_txn_pair_distinct_docs() {
    edge::compat(async {
        let cluster = build_cluster().await;
        let node = cluster.client(0);

        node.schema_add("type Snap { status: String  content: String }")
            .expect("schema add");

        let mut ids = Vec::new();
        for i in 0..2 {
            let data = node
                .query(&format!(
                    r#"mutation {{ add_Snap(input: {{status: "streaming", content: "init-{i}"}}) {{ _docID }} }}"#
                ))
                .expect("create doc");
            ids.push(data["add_Snap"][0]["_docID"].as_str().unwrap().to_string());
        }

        let tx1 = node.tx_create().expect("tx1 create");
        let tx2 = node.tx_create().expect("tx2 create");

        node.query_with_tx(
            &format!(
                r#"mutation {{ update_Snap(docID: "{}", input: {{status: "streaming"}}) {{ _docID }} }}"#,
                ids[0]
            ),
            &tx1,
        )
        .expect("tx1 update doc0");
        node.query_with_tx(
            &format!(
                r#"mutation {{ update_Snap(docID: "{}", input: {{status: "streaming"}}) {{ _docID }} }}"#,
                ids[1]
            ),
            &tx2,
        )
        .expect("tx2 update doc1");

        node.tx_commit(&tx1).expect("tx1 commit");
        node.tx_commit(&tx2).expect(
            "tx2 commit: identical-content block writes to distinct documents must not conflict",
        );
    })
    .await;
}

/// Control: two interactive transactions updating the SAME document remain a
/// genuine conflict — the second commit must abort.
#[nuclei::test(threads = 4)]
async fn issue1194_same_doc_txn_pair_still_conflicts() {
    edge::compat(async {
        let cluster = build_cluster().await;
        let node = cluster.client(0);

        node.schema_add("type Snap { status: String  content: String }")
            .expect("schema add");

        let data = node
            .query(r#"mutation { add_Snap(input: {status: "streaming", content: "init"}) { _docID } }"#)
            .expect("create doc");
        let id = data["add_Snap"][0]["_docID"].as_str().unwrap().to_string();

        let tx1 = node.tx_create().expect("tx1 create");
        let tx2 = node.tx_create().expect("tx2 create");

        node.query_with_tx(
            &format!(
                r#"mutation {{ update_Snap(docID: "{id}", input: {{content: "a"}}) {{ _docID }} }}"#
            ),
            &tx1,
        )
        .expect("tx1 update");
        node.query_with_tx(
            &format!(
                r#"mutation {{ update_Snap(docID: "{id}", input: {{content: "b"}}) {{ _docID }} }}"#
            ),
            &tx2,
        )
        .expect("tx2 update");

        node.tx_commit(&tx1).expect("tx1 commit");
        let err = node
            .tx_commit(&tx2)
            .expect_err("tx2 commit must conflict: same document, overlapping snapshots");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("transaction conflict"),
            "expected a transaction conflict, got: {msg}"
        );
    })
    .await;
}

/// Unconditional ID updates whose output is only `_docID` depend on the
/// patched fields (and any index/embedding dependencies), not the whole
/// document. Disjoint changes to one document can therefore both commit.
#[nuclei::test(threads = 4)]
async fn issue1194_same_doc_disjoint_field_txn_pair_commits() {
    edge::compat(async {
        let cluster = build_cluster().await;
        let node = cluster.client(0);

        node.schema_add("type Snap { status: String! content: String! untouched: String! }")
            .expect("schema add");

        let data = node
            .query(r#"mutation { add_Snap(input: {status: "streaming", content: "init", untouched: "preserved"}) { _docID } }"#)
            .expect("create doc");
        let id = data["add_Snap"][0]["_docID"].as_str().unwrap().to_string();

        let tx1 = node.tx_create().expect("tx1 create");
        let tx2 = node.tx_create().expect("tx2 create");

        node.query_with_tx(
            &format!(
                r#"mutation {{ update_Snap(docID: "{id}", input: {{content: "first"}}) {{ _docID }} }}"#
            ),
            &tx1,
        )
        .expect("tx1 update content");
        node.query_with_tx(
            &format!(
                r#"mutation {{ update_Snap(docID: "{id}", input: {{status: "claimed"}}) {{ _docID }} }}"#
            ),
            &tx2,
        )
        .expect("tx2 update status");

        node.tx_commit(&tx1).expect("tx1 commit");
        node.tx_commit(&tx2)
            .expect("disjoint projected field dependencies should commit");

        let final_doc = node
            .query(&format!(
                r#"query {{ Snap(docID: "{id}") {{ status content untouched }} }}"#
            ))
            .expect("read merged document");
        assert_eq!(final_doc["Snap"][0]["content"], json!("first"));
        assert_eq!(final_doc["Snap"][0]["status"], json!("claimed"));
        assert_eq!(final_doc["Snap"][0]["untouched"], json!("preserved"));
    })
    .await;
}

/// A predicate read remains part of the transaction's dependency set, even if
/// the mutation returns only `_docID`.
#[nuclei::test(threads = 4)]
async fn issue1194_filtered_update_still_conflicts_on_predicate_field() {
    edge::compat(async {
        let cluster = build_cluster().await;
        let node = cluster.client(0);
        node.schema_add("type Snap { status: String  content: String }")
            .expect("schema add");
        let data = node
            .query(r#"mutation { add_Snap(input: {status: "streaming", content: "init"}) { _docID } }"#)
            .expect("create doc");
        let id = data["add_Snap"][0]["_docID"].as_str().unwrap().to_string();
        let tx1 = node.tx_create().expect("tx1 create");
        let tx2 = node.tx_create().expect("tx2 create");

        node.query_with_tx(
            &format!(r#"mutation {{ update_Snap(filter: {{_docID: {{_eq: "{id}"}}, status: {{_eq: "streaming"}}}}, input: {{content: "first"}}) {{ _docID }} }}"#),
            &tx1,
        )
        .expect("filtered tx1 update");
        node.query_with_tx(
            &format!(r#"mutation {{ update_Snap(docID: "{id}", input: {{status: "claimed"}}) {{ _docID }} }}"#),
            &tx2,
        )
        .expect("tx2 update predicate field");

        node.tx_commit(&tx2).expect("tx2 commit");
        let err = node
            .tx_commit(&tx1)
            .expect_err("predicate change must conflict");
        assert!(format!("{err:#}").contains("transaction conflict"));
    })
    .await;
}

/// Selecting a field in the mutation response makes it a read dependency; the
/// projected fast path is limited to `_docID`-only results.
#[nuclei::test(threads = 4)]
async fn issue1194_selected_output_field_still_conflicts() {
    edge::compat(async {
        let cluster = build_cluster().await;
        let node = cluster.client(0);
        node.schema_add("type Snap { status: String  content: String }")
            .expect("schema add");
        let data = node
            .query(r#"mutation { add_Snap(input: {status: "streaming", content: "init"}) { _docID } }"#)
            .expect("create doc");
        let id = data["add_Snap"][0]["_docID"].as_str().unwrap().to_string();
        let tx1 = node.tx_create().expect("tx1 create");
        let tx2 = node.tx_create().expect("tx2 create");

        node.query_with_tx(
            &format!(r#"mutation {{ update_Snap(docID: "{id}", input: {{status: "claimed"}}) {{ content }} }}"#),
            &tx1,
        )
        .expect("tx1 update with selected content");
        node.query_with_tx(
            &format!(r#"mutation {{ update_Snap(docID: "{id}", input: {{content: "changed"}}) {{ _docID }} }}"#),
            &tx2,
        )
        .expect("tx2 update selected field");

        node.tx_commit(&tx2).expect("tx2 commit");
        let err = node
            .tx_commit(&tx1)
            .expect_err("selected output change must conflict");
        assert!(format!("{err:#}").contains("transaction conflict"));
    })
    .await;
}

/// Composite index maintenance depends on every indexed component, including
/// components the mutation did not patch.
#[nuclei::test(threads = 4)]
async fn issue1194_composite_index_component_still_conflicts() {
    edge::compat(async {
        let cluster = build_cluster().await;
        let node = cluster.client(0);
        node.schema_add(
            r#"type Snap @index(includes: [{field: "status", direction: ASC}, {field: "content", direction: ASC}]) { status: String content: String }"#,
        )
        .expect("schema add");
        let data = node
            .query(r#"mutation { add_Snap(input: {status: "streaming", content: "init"}) { _docID } }"#)
            .expect("create doc");
        let id = data["add_Snap"][0]["_docID"].as_str().unwrap().to_string();
        let tx1 = node.tx_create().expect("tx1 create");
        let tx2 = node.tx_create().expect("tx2 create");

        node.query_with_tx(
            &format!(
                r#"mutation {{ update_Snap(docID: "{id}", input: {{content: "first"}}) {{ _docID }} }}"#
            ),
            &tx1,
        )
        .expect("tx1 update composite component");
        node.query_with_tx(
            &format!(r#"mutation {{ update_Snap(docID: "{id}", input: {{status: "claimed"}}) {{ _docID }} }}"#),
            &tx2,
        )
        .expect("tx2 update other composite component");

        node.tx_commit(&tx1).expect("tx1 commit");
        let err = node
            .tx_commit(&tx2)
            .expect_err("changed composite index dependency must conflict");
        assert!(format!("{err:#}").contains("transaction conflict"));
    })
    .await;
}

/// Projection is local to the supported mutation read. A normal query later in
/// that same explicit transaction must still register its full read set.
#[nuclei::test(threads = 4)]
async fn issue1194_later_full_read_still_conflicts() {
    edge::compat(async {
        let cluster = build_cluster().await;
        let node = cluster.client(0);
        node.schema_add("type Snap { status: String  content: String }")
            .expect("schema add");
        let data = node
            .query(r#"mutation { add_Snap(input: {status: "streaming", content: "init"}) { _docID } }"#)
            .expect("create doc");
        let id = data["add_Snap"][0]["_docID"].as_str().unwrap().to_string();
        let tx1 = node.tx_create().expect("tx1 create");
        let tx2 = node.tx_create().expect("tx2 create");

        node.query_with_tx(
            &format!(
                r#"mutation {{ update_Snap(docID: "{id}", input: {{content: "first"}}) {{ _docID }} }}"#
            ),
            &tx1,
        )
        .expect("projected tx1 update");
        node.query_with_tx(
            &format!(r#"query {{ Snap(docID: "{id}") {{ status }} }}"#),
            &tx1,
        )
        .expect("ordinary tx1 read");
        node.query_with_tx(
            &format!(r#"mutation {{ update_Snap(docID: "{id}", input: {{status: "claimed"}}) {{ _docID }} }}"#),
            &tx2,
        )
        .expect("tx2 update");

        node.tx_commit(&tx2).expect("tx2 commit");
        let err = node
            .tx_commit(&tx1)
            .expect_err("later full read must still conflict");
        assert!(format!("{err:#}").contains("transaction conflict"));
    })
    .await;
}
