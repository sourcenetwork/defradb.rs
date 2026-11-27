//! A node with P2P on and no peer announces each write to no one. Go
//! publishes such a write silently, so the node logs no warning or error for
//! it (#2102). A write whose document topic has no peer but whose collection
//! topic does is announced on the collection topic, also at DEBUG (#3388).
//!
//! testo nodes run in the test process and leave no operator log, so this
//! reads the stdout log of a real `defradb start`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use integration_test::{extract_p2p_addr, poll_until, TestCluster};

const NOT_ANNOUNCED: &str = "No peer subscribed to the write's topics; nothing to announce";
const ANNOUNCED_ON_COLLECTION: &str =
    "No peer subscribed to the document topic; announced on the collection topic";
const BROADCAST_TARGETS: [&str; 2] = ["p2p::sync::broadcaster", "db::merge::broadcast_mutator"];
const WRITES: usize = 4;

fn stdout_log(cluster: &TestCluster, node: usize) -> PathBuf {
    cluster.nodes[node]
        .rootdir
        .parent()
        .expect("node rootdir has parent")
        .join("logs/stdout.log")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn broadcast_complaints(log: &str) -> Vec<&str> {
    log.lines()
        .filter(|line| line.contains(" WARN ") || line.contains(" ERROR "))
        .filter(|line| {
            BROADCAST_TARGETS.iter().any(|target| line.contains(target))
                || line.contains("InsufficientPeers")
        })
        .collect()
}

#[nuclei::test]
#[mantra_macros::req_test("defra.p2p.peerless_broadcast")]
async fn rust_p2p_writes_without_peers_log_no_broadcast_error() {
    // The node reads RUST_LOG before --log-level, and CI sets it to info, so
    // the debug lines this test counts are asked for here. nextest runs each
    // test in its own process, so no other test sees it.
    std::env::set_var("RUST_LOG", "debug");
    edge::compat(async {
        let cluster = TestCluster::builder()
            .rust_nodes(1)
            .with_p2p()
            // No ANSI escapes, so a log line still reads " WARN ".
            .with_extra_rust_args(["--log-level", "debug", "--no-log-color"])
            .build()
            .await
            .unwrap();
        cluster
            .wait_for_log(0, "p2p_listening", Duration::from_secs(15))
            .await
            .expect("P2P listener did not start");
        let client = cluster.client(0);
        client
            .schema_add("type User { name: String  age: Int }")
            .expect("add schema");

        let created = client
            .query(
                r#"mutation {
                    a: add_User(input: {name: "Ada", age: 36}) { _docID }
                    b: add_User(input: {name: "Bob", age: 41}) { _docID }
                }"#,
            )
            .expect("batched create");
        client
            .query(r#"mutation { add_User(input: {name: "Cy", age: 7}) { _docID } }"#)
            .expect("single create");
        let doc_id = created["a"][0]["_docID"].as_str().expect("docID");
        client
            .query(&format!(
                r#"mutation {{ update_User(docID: "{doc_id}", input: {{age: 37}}) {{ _docID }} }}"#
            ))
            .expect("update");

        let log = stdout_log(&cluster, 0);
        poll_until(
            || {
                let log = read(&log);
                !broadcast_complaints(&log).is_empty()
                    || log.matches(NOT_ANNOUNCED).count() >= WRITES
            },
            Duration::from_secs(30),
            Duration::from_millis(100),
            "every write's broadcast to have finished",
        )
        .await;

        let log = read(&log);
        let complaints = broadcast_complaints(&log);
        assert!(
            complaints.is_empty(),
            "a peerless node logged its broadcasts as failures:\n{}",
            complaints.join("\n")
        );
        assert!(
            log.matches(NOT_ANNOUNCED).count() >= WRITES,
            "expected {WRITES} unannounced writes at debug level"
        );
    })
    .await;
}

/// A peer that subscribes to the collection topic alone is the usual case
/// until it subscribes to a document: the write is announced there, reaching
/// the peer, and the writer logs the missing document-topic peer at DEBUG,
/// not as a failure.
#[nuclei::test]
#[mantra_macros::req_test("defra.p2p.peerless_broadcast")]
async fn rust_p2p_write_announced_on_the_collection_topic_alone_logs_no_broadcast_error() {
    std::env::set_var("RUST_LOG", "debug");
    edge::compat(async {
        let cluster = TestCluster::builder()
            .rust_nodes(2)
            .with_p2p()
            .with_extra_rust_args(["--log-level", "debug", "--no-log-color"])
            .build()
            .await
            .unwrap();
        for node in [0, 1] {
            cluster
                .wait_for_log(node, "p2p_listening", Duration::from_secs(15))
                .await
                .unwrap_or_else(|_| panic!("node{node} P2P listener did not start"));
        }
        let writer = cluster.client(0);
        let reader = cluster.client(1);
        writer
            .schema_add("type User { name: String  age: Int }")
            .expect("add schema on the writer");
        reader
            .schema_add("type User { name: String  age: Int }")
            .expect("add schema on the reader");

        let reader_addr = extract_p2p_addr(&cluster, 1);
        writer
            .p2p_connect(&[reader_addr.as_str()])
            .expect("connect the peers");
        // Both nodes follow the collection and neither follows a document, so
        // the write's document topic has no peer and its collection topic
        // does. That the peer receives it is covered in
        // `net::peerless_writes`; this reads the writer's own log, which says
        // the announce went out on the collection topic alone, at DEBUG.
        writer
            .p2p_collection_add(&["User"])
            .expect("the writer subscribes to the collection");
        reader
            .p2p_collection_add(&["User"])
            .expect("the reader subscribes to the collection");
        writer
            .query(r#"mutation { add_User(input: {name: "Ada", age: 36}) { _docID } }"#)
            .expect("create");

        let log = stdout_log(&cluster, 0);
        poll_until(
            || {
                let log = read(&log);
                !broadcast_complaints(&log).is_empty()
                    || log.matches(ANNOUNCED_ON_COLLECTION).count() >= 1
            },
            Duration::from_secs(30),
            Duration::from_millis(100),
            "the document-topic announce to be logged",
        )
        .await;

        let log = read(&log);
        let complaints = broadcast_complaints(&log);
        assert!(
            complaints.is_empty(),
            "a write announced on the collection topic alone was logged as a failure:\n{}",
            complaints.join("\n")
        );
        assert!(
            log.matches(ANNOUNCED_ON_COLLECTION).count() >= 1,
            "expected the missing document-topic peer at debug level"
        );
    })
    .await;
}
