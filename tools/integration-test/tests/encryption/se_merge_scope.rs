//! SE coverage for documents a node MERGED rather than wrote (spike L1).
//!
//! `se_cross_runtime.rs` only covers owner == writer. Go additionally
//! regenerates SE artifacts for every document it merges and pushes them to
//! its own replicators (`internal/db/p2p/p2p.go:700-713` re-enters
//! `SendUpdate{IsRelay:true}` -> `pushLogToReplicators` ->
//! `internal/se/coordinator.go:246` `HandlePushToReplicators`). Rust generates
//! merge artifacts into its OWN store only
//! (`crates/db/src/merge/merge_handler/composite_persist.rs:171-193`) and never
//! pushes them, while its `encrypted_<Col>` query resolves exclusively through
//! replicators (`crates/db/src/merge/se_query_transport.rs:142-167`).
//!
//! Topology for every test: CREATOR -> MIDDLE -> TAIL, a replicator chain.
//! MIDDLE merges the document and is the node the `encrypted_User` query runs
//! on; TAIL is MIDDLE's only replicator and therefore the only node that can
//! answer MIDDLE's SE query.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use integration_test::{BinarySource, TestCluster};

const SHARED_SE_KEY: [u8; 32] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
    0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
];

const USER_SCHEMA: &str = "type User { name: String  age: Int  city: String }";

/// The Go compat binary the spike pane provides; `defradb` is not on PATH.
fn go_binary() -> PathBuf {
    if let Ok(p) = std::env::var("DEFRA_GO_BIN") {
        return PathBuf::from(p);
    }
    PathBuf::from(std::env::var("HOME").expect("HOME"))
        .join(".cache/defra-harness/53f0e76a3/defradb")
}

fn p2p_addr(cluster: &TestCluster, idx: usize) -> String {
    cluster
        .client(idx)
        .p2p_info()
        .expect("p2p info")
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .expect("no P2P address")
        .to_string()
}

/// Wire CREATOR -> MIDDLE -> TAIL as a replicator chain over `collection`.
async fn build_chain(cluster: &TestCluster, creator: usize, middle: usize, tail: usize) {
    let timeout = Duration::from_secs(30);
    for idx in [creator, middle, tail] {
        cluster
            .wait_for_log(idx, "p2p_listening", timeout)
            .await
            .unwrap_or_else(|e| panic!("node {idx} P2P listener did not start: {e}"));
        let c = cluster.client(idx);
        c.schema_add(USER_SCHEMA)
            .unwrap_or_else(|e| panic!("schema on node {idx}: {e}"));
        c.encrypted_index_add("User", "name")
            .unwrap_or_else(|e| panic!("encrypted index on node {idx}: {e}"));
        c.p2p_collection_add(&["User"])
            .unwrap_or_else(|e| panic!("collection add on node {idx}: {e}"));
    }

    let middle_addr = p2p_addr(cluster, middle);
    let tail_addr = p2p_addr(cluster, tail);

    cluster
        .client(creator)
        .p2p_connect(&[&middle_addr])
        .expect("creator connect middle");
    cluster
        .client(middle)
        .p2p_connect(&[&tail_addr])
        .expect("middle connect tail");
    cluster
        .client(creator)
        .p2p_replicator_set(&["User"], &middle_addr)
        .expect("replicator creator->middle");
    cluster
        .client(middle)
        .p2p_replicator_set(&["User"], &tail_addr)
        .expect("replicator middle->tail");
}

fn create_user(cluster: &TestCluster, idx: usize, name: &str) -> String {
    let created = cluster
        .client(idx)
        .query(&format!(
            r#"mutation {{ add_User(input: {{name: "{name}", age: 21, city: "NYC"}}) {{ _docID }} }}"#
        ))
        .expect("create User");
    created["add_User"][0]["_docID"]
        .as_str()
        .or_else(|| created["add_User"]["_docID"].as_str())
        .expect("missing _docID")
        .to_string()
}

/// Same `name` (so the SE tag is identical) but a different `age`, so the two
/// documents get distinct content-addressed docIDs.
fn create_user_aged(cluster: &TestCluster, idx: usize, name: &str, age: u32) -> String {
    let created = cluster
        .client(idx)
        .query(&format!(
            r#"mutation {{ add_User(input: {{name: "{name}", age: {age}, city: "NYC"}}) {{ _docID }} }}"#
        ))
        .expect("create User");
    created["add_User"][0]["_docID"]
        .as_str()
        .or_else(|| created["add_User"]["_docID"].as_str())
        .expect("missing _docID")
        .to_string()
}

/// Poll a plain (non-SE) read on `idx` until `doc_id` is present.
async fn wait_for_merge(cluster: &TestCluster, idx: usize, doc_id: &str, deadline: Instant) {
    loop {
        let res = cluster
            .client(idx)
            .query("query { User { _docID } }")
            .expect("plain User query");
        if let Some(rows) = res["User"].as_array() {
            if rows.iter().any(|r| r["_docID"].as_str() == Some(doc_id)) {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "document {doc_id} never merged onto node {idx}; last: {res}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Poll `encrypted_User` on `idx`; true once `doc_id` comes back.
async fn se_query_hits(
    cluster: &TestCluster,
    idx: usize,
    doc_id: &str,
    name: &str,
    deadline: Instant,
) -> bool {
    let query =
        format!(r#"query {{ encrypted_User(filter: {{name: {{_eq: "{name}"}}}}) {{ docIDs }} }}"#);
    loop {
        if let Ok(res) = cluster.client(idx).query(&query) {
            if let Some(rows) = res["encrypted_User"].as_array() {
                let hit = rows.iter().any(|r| {
                    r["docIDs"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|v| v.as_str() == Some(doc_id)))
                });
                if hit {
                    return true;
                }
            }
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// All-Rust chain. Both write and query identities are `None`
/// (`crates/cli/src/commands/start/server_p2p/libp2p.rs:145,429-438`), so a
/// miss here CANNOT be a tag mismatch: it isolates the missing
/// regenerate-and-push-on-merge.
#[tokio::test]
async fn rust_merger_se_query_finds_replicated_doc() {
    let cluster = TestCluster::builder()
        .rust_nodes(3)
        .with_p2p()
        .with_encryption()
        .with_shared_searchable_encryption_key(SHARED_SE_KEY)
        .build()
        .await
        .expect("build 3-node rust cluster");

    build_chain(&cluster, 0, 1, 2).await;
    let doc_id = create_user(&cluster, 0, "John");
    wait_for_merge(
        &cluster,
        1,
        &doc_id,
        Instant::now() + Duration::from_secs(45),
    )
    .await;

    let hit = se_query_hits(
        &cluster,
        1,
        &doc_id,
        "John",
        Instant::now() + Duration::from_secs(45),
    )
    .await;
    assert!(
        hit,
        "rust merger did not resolve a merged doc through its own SE query"
    );
}

/// Go CREATOR (node 2) -> Rust MIDDLE (node 0) -> Rust TAIL (node 1).
#[tokio::test]
async fn go_creator_rust_merger_se_query_finds_doc() {
    let cluster = TestCluster::builder()
        .rust_nodes(2)
        .go_nodes(1)
        .with_go_binary(BinarySource::Path(go_binary()))
        .with_p2p()
        .with_encryption()
        .with_shared_searchable_encryption_key(SHARED_SE_KEY)
        .build()
        .await
        .expect("build 2-rust + 1-go cluster");

    build_chain(&cluster, 2, 0, 1).await;
    let doc_id = create_user(&cluster, 2, "John");
    wait_for_merge(
        &cluster,
        0,
        &doc_id,
        Instant::now() + Duration::from_secs(60),
    )
    .await;

    let hit = se_query_hits(
        &cluster,
        0,
        &doc_id,
        "John",
        Instant::now() + Duration::from_secs(45),
    )
    .await;
    assert!(
        hit,
        "rust merger did not resolve a Go-created merged doc through its own SE query"
    );
}

/// The mirror: Rust CREATOR (node 0) -> Go MIDDLE (node 1) -> Go TAIL (node 2).
#[tokio::test]
async fn go_merger_se_query_finds_rust_created_doc() {
    let cluster = TestCluster::builder()
        .rust_nodes(1)
        .go_nodes(2)
        .with_go_binary(BinarySource::Path(go_binary()))
        .with_p2p()
        .with_encryption()
        .with_shared_searchable_encryption_key(SHARED_SE_KEY)
        .build()
        .await
        .expect("build 1-rust + 2-go cluster");

    build_chain(&cluster, 0, 1, 2).await;
    let doc_id = create_user(&cluster, 0, "John");
    wait_for_merge(
        &cluster,
        1,
        &doc_id,
        Instant::now() + Duration::from_secs(60),
    )
    .await;

    let hit = se_query_hits(
        &cluster,
        1,
        &doc_id,
        "John",
        Instant::now() + Duration::from_secs(45),
    )
    .await;
    assert!(
        hit,
        "go merger did not resolve a Rust-created merged doc through its own SE query"
    );
}

/// Separates the defect from the soak generator's 40-name pool
/// (`133-data-generation-review.md` Part 2, item 4), without a soak run.
///
/// One name, two documents, one query, on one node: MIDDLE writes `B` itself
/// and merges `A` from CREATOR. Both carry `name: "John"`, so the query tag is
/// identical for both and every pool-independent explanation (SE key, index,
/// tag bytes, query wiring, transport) is controlled for by `B`.
///
/// If `B` hits and `A` misses, the miss is per-document and the pool only
/// multiplies how many `A`-like documents one query has to name: pool size
/// changes the reported rate, not the defect.
#[tokio::test]
async fn merged_and_self_written_docs_both_hit_on_same_name() {
    let cluster = TestCluster::builder()
        .rust_nodes(3)
        .with_p2p()
        .with_encryption()
        .with_shared_searchable_encryption_key(SHARED_SE_KEY)
        .build()
        .await
        .expect("build 3-node rust cluster");

    build_chain(&cluster, 0, 1, 2).await;

    let merged = create_user_aged(&cluster, 0, "John", 21);
    let self_written = create_user_aged(&cluster, 1, "John", 22);
    wait_for_merge(
        &cluster,
        1,
        &merged,
        Instant::now() + Duration::from_secs(45),
    )
    .await;

    // Control: MIDDLE's own write must be resolvable through its replicator.
    let control_hit = se_query_hits(
        &cluster,
        1,
        &self_written,
        "John",
        Instant::now() + Duration::from_secs(45),
    )
    .await;
    assert!(
        control_hit,
        "control failed: MIDDLE cannot resolve a document it wrote itself, \
         so this test cannot say anything about the merged one"
    );

    let merged_hit = se_query_hits(
        &cluster,
        1,
        &merged,
        "John",
        Instant::now() + Duration::from_secs(30),
    )
    .await;

    let ids = cluster
        .client(1)
        .query(r#"query { encrypted_User(filter: {name: {_eq: "John"}}) { docIDs } }"#)
        .expect("se query");
    assert!(
        merged_hit,
        "one name, two documents: MIDDLE resolved its own write {self_written} \
         but not the merged {merged}; query returned {ids}"
    );
}
