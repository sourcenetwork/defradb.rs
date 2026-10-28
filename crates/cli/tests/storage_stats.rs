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

#[nuclei::test]
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
    let users = db.get_collection("Users").unwrap().unwrap().schema_arc();
    let mut doc = document::Document::new(users);
    doc.set(
        "name",
        document::NormalValue::String("private value".into()),
    )
    .unwrap();
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
fn storage_stats_rejects_memory() {
    let root = tempfile::tempdir().unwrap();
    let mut config = cli::config::Config {
        rootdir: root.path().to_path_buf(),
        ..Default::default()
    };
    config.datastore.store = cli::config::DatastoreType::Memory;
    std::fs::write(
        root.path().join("config.yaml"),
        serde_yaml::to_string(&config).unwrap(),
    )
    .unwrap();
    let output = command(root.path()).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("on-disk database"));
    assert!(!config.data_path().exists());
}

#[nuclei::test]
async fn storage_stats_reads_engine_encryption_without_replacing_key() {
    use keyring::{FileKeyring, Keyring, ENCRYPTION_KEY, KEYRING_SECRET_ENV};
    let root = tempfile::tempdir().unwrap();
    let mut config = cli::config::Config {
        rootdir: root.path().to_path_buf(),
        ..Default::default()
    };
    config.datastore.at_rest_encryption = true;
    let secret = "storage-stats-secret";
    let keyring = FileKeyring::open(config.keyring_path(), secret.as_bytes()).unwrap();
    keyring.set(ENCRYPTION_KEY, &[42; 32]).unwrap();
    std::fs::write(
        root.path().join("config.yaml"),
        serde_yaml::to_string(&config).unwrap(),
    )
    .unwrap();
    let database = db::DB::new(
        storage::RegolithStore::open_with_options(
            config.data_path(),
            storage::RegolithStoreOptions::default().with_encryption_key([42; 32]),
        )
        .unwrap(),
    )
    .unwrap();
    database
        .create_collections_atomic(query::parse_sdl("type Users { name: String }").unwrap())
        .await
        .unwrap();
    database.close().await.unwrap();
    drop(database);
    let output = command(root.path())
        .env(KEYRING_SECRET_ENV, secret)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["total"]["keys"].as_u64().unwrap() > 0);
    assert_eq!(keyring.get(ENCRYPTION_KEY).unwrap().as_slice(), &[42; 32]);
}

#[test]
fn storage_stats_does_not_initialize_a_missing_database() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    let output = command(&missing).output().unwrap();
    assert!(!output.status.success());
    assert!(!missing.exists());
}

#[test]
fn encrypted_storage_stats_does_not_generate_a_missing_key() {
    use keyring::{FileKeyring, Keyring, ENCRYPTION_KEY, KEYRING_SECRET_ENV};
    let root = tempfile::tempdir().unwrap();
    let mut config = cli::config::Config {
        rootdir: root.path().to_path_buf(),
        ..Default::default()
    };
    config.datastore.at_rest_encryption = true;
    let secret = "storage-stats-missing-key";
    let keyring = FileKeyring::open(config.keyring_path(), secret.as_bytes()).unwrap();
    std::fs::write(
        root.path().join("config.yaml"),
        serde_yaml::to_string(&config).unwrap(),
    )
    .unwrap();
    let output = command(root.path())
        .env(KEYRING_SECRET_ENV, secret)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(matches!(
        keyring.get(ENCRYPTION_KEY),
        Err(keyring::Error::NotFound(_))
    ));
    assert!(!config.data_path().exists());
}

/// A key rotation through the keyring: the store was sealed under the key now
/// kept as a previous key, and stays readable while the current key is new.
#[nuclei::test]
async fn storage_stats_reads_a_store_sealed_under_a_previous_key() {
    use keyring::{
        FileKeyring, Keyring, ENCRYPTION_KEY, KEYRING_SECRET_ENV, PREVIOUS_ENCRYPTION_KEYS,
    };
    let root = tempfile::tempdir().unwrap();
    let mut config = cli::config::Config {
        rootdir: root.path().to_path_buf(),
        ..Default::default()
    };
    config.datastore.at_rest_encryption = true;
    let secret = "storage-stats-rotation";
    std::fs::write(
        root.path().join("config.yaml"),
        serde_yaml::to_string(&config).unwrap(),
    )
    .unwrap();
    let database = db::DB::new(
        storage::RegolithStore::open_with_options(
            config.data_path(),
            storage::RegolithStoreOptions::default().with_encryption_key([42; 32]),
        )
        .unwrap(),
    )
    .unwrap();
    database
        .create_collections_atomic(query::parse_sdl("type Users { name: String }").unwrap())
        .await
        .unwrap();
    database.close().await.unwrap();
    drop(database);

    let keyring = FileKeyring::open(config.keyring_path(), secret.as_bytes()).unwrap();
    keyring.set(ENCRYPTION_KEY, &[43; 32]).unwrap();
    let stats = || {
        command(root.path())
            .env(KEYRING_SECRET_ENV, secret)
            .output()
            .unwrap()
    };
    assert!(
        !stats().status.success(),
        "the new key alone opened the store"
    );
    keyring
        .set(PREVIOUS_ENCRYPTION_KEYS, &[[41; 32], [42; 32]].concat())
        .unwrap();
    let output = stats();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    keyring.set(PREVIOUS_ENCRYPTION_KEYS, &[42; 31]).unwrap();
    let output = stats();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("32-byte keys"));
}
