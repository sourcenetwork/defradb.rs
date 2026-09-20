use std::process::Command;

fn command(root: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_defra"));
    command
        .args(["--rootdir"])
        .arg(root)
        .args(["storage", "stats"]);
    command
}

#[tokio::test]
async fn storage_stats_prints_aggregates_without_creating_config_or_keys() {
    let root = tempfile::tempdir().unwrap();
    let config = cli::config::Config {
        rootdir: root.path().to_path_buf(),
        ..Default::default()
    };
    let db = db::DB::new(storage::RegolithStore::open(config.data_path()).unwrap()).unwrap();
    db.create_collections_atomic(query::parse_sdl("type Users { name: String }").unwrap())
        .await
        .unwrap();
    db.close().await.unwrap();
    drop(db);
    let output = command(root.path()).arg("--versions").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["total"]["keys"].as_u64().unwrap() > 0);
    assert!(!root.path().join("config.yaml").exists());
    assert!(!config.keyring_path().exists());
}

#[test]
fn storage_stats_does_not_initialize_a_missing_database() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    let output = command(&missing).output().unwrap();
    assert!(!output.status.success());
    assert!(!missing.exists());
}
