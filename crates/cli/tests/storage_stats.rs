use query::mutator::DocMutator;
use std::process::Command;
use std::sync::Arc;

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
    let db =
        Arc::new(db::DB::new(storage::RegolithStore::open(config.data_path()).unwrap()).unwrap());
    db.create_collections_atomic(query::parse_sdl("type Users { name: String }").unwrap())
        .await
        .unwrap();
    let mutator = db::DbDocMutator::new(db.clone(), db.new_txn(false).await.unwrap());
    let mut doc = document::Document::new();
    doc.set(
        "name",
        document::NormalValue::String("private value".into()),
    );
    mutator.create("Users", doc).await.unwrap();
    mutator.take_txn().await.unwrap().commit().await.unwrap();
    drop(mutator);
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
    let collection = report["collections"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(collection["fields"]["name"]["documents"], 1);
    assert_eq!(collection["fields"]["name"]["versions"], 1);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private value"));
    assert!(!root.path().join("config.yaml").exists());
    assert!(!config.keyring_path().exists());
}

#[test]
fn storage_stats_rejects_memory_and_encrypted_stores() {
    for encrypted in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut config = cli::config::Config {
            rootdir: root.path().to_path_buf(),
            ..Default::default()
        };
        config.datastore.at_rest_encryption = encrypted;
        if !encrypted {
            config.datastore.store = cli::config::DatastoreType::Memory;
        }
        std::fs::write(
            root.path().join("config.yaml"),
            serde_yaml::to_string(&config).unwrap(),
        )
        .unwrap();
        let output = command(root.path()).output().unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        let expected = if encrypted {
            "encrypted store"
        } else {
            "on-disk database"
        };
        assert!(error.contains(expected), "{error}");
        assert!(!config.data_path().exists());
    }
}

#[test]
fn storage_stats_does_not_initialize_a_missing_database() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    let output = command(&missing).output().unwrap();
    assert!(!output.status.success());
    assert!(!missing.exists());
}
