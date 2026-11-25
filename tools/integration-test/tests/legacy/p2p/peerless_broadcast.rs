//! A node with P2P on and no peer announces each write to no one. Go
//! publishes such a write silently, so the node logs no warning or error for
//! it (#2102).
//!
//! testo nodes run in the test process and leave no operator log, so this
//! reads the stdout log of a real `defradb start`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use integration_test::{poll_until, TestCluster};

const NOT_ANNOUNCED: &str = "No peer subscribed to the write's topics; nothing to announce";
const BROADCAST_TARGETS: [&str; 2] = ["p2p::sync::broadcaster", "db::merge::broadcast_mutator"];
const WRITES: usize = 4;

fn stdout_log(cluster: &TestCluster) -> PathBuf {
    cluster.nodes[0]
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
    edge::compat(async {
        let cluster = TestCluster::builder()
            .rust_nodes(1)
            .with_p2p()
            .with_extra_rust_args(["--log-level", "debug"])
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

        let log = stdout_log(&cluster);
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
        // The batch reports a failed broadcast right after it returns.
        nuclei::time::sleep(Duration::from_millis(500)).await;

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
