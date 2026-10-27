//! A backup leaves chunks out, and importing it cuts the texts again.

use std::process::Command;

use integration_test::TestCluster;

use super::support::{body, create, listed, visible_hashes, SCHEMA};

// The client must also run in development mode for backup commands.
fn backup(cluster: &TestCluster, node: usize, args: &[&str]) {
    let client_dir = tempfile::tempdir().unwrap();
    let output = Command::new(cluster.client(node).binary_path())
        .arg("--rootdir")
        .arg(client_dir.path())
        .arg("--url")
        .arg(&cluster.nodes[node].http_addr)
        .arg("--development")
        .arg("client")
        .arg("backup")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn a_backup_round_trip_rebuilds_the_chunks() {
    let cluster = TestCluster::builder()
        .rust_nodes(2)
        .with_development()
        .build()
        .await
        .unwrap();
    let (source, target) = (cluster.client(0), cluster.client(1));
    source.schema_add(SCHEMA).unwrap();
    target.schema_add(SCHEMA).unwrap();
    let doc_id = create(&source, &body(5, "harbours"));

    let file = format!("{}/backup.json", cluster.nodes[0].rootdir.display());
    backup(&cluster, 0, &["export", &file]);
    let exported = std::fs::read_to_string(&file).unwrap();
    assert!(!exported.contains("__body_chunk"), "{exported}");
    assert!(!exported.contains("_body_chunks"), "{exported}");

    backup(&cluster, 1, &["import", &file]);
    assert_eq!(visible_hashes(&target, None), visible_hashes(&source, None));
    assert_eq!(
        listed(&target, &doc_id, None),
        listed(&source, &doc_id, None)
    );
}
