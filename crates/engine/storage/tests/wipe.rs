//! `storage::wipe` empties the data directory, removes the owned
//! directories and never reaches a kept path.

use storage::corekv::{Reader, Store, Writer};
use storage::{RegolithStore, RegolithStoreOptions, WipeTarget};

#[test]
fn wipe_empties_the_store_and_removes_only_owned_dirs() {
    workers::block_on(async {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let keys = root.path().join("keys");
        let vera = root.path().join("vera");
        std::fs::create_dir_all(&keys).unwrap();
        std::fs::create_dir_all(&vera).unwrap();
        std::fs::write(vera.join("chain"), b"old").unwrap();

        let store =
            RegolithStore::open_with_options(&data, RegolithStoreOptions::default()).unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        txn.set(b"key", b"value").await.unwrap();
        txn.commit().await.unwrap();
        store.close().await.unwrap();
        drop(store);

        storage::wipe(&WipeTarget {
            store: Some((data.clone(), RegolithStoreOptions::default())),
            owned_dirs: vec![vera.clone()],
            kept: vec![keys.clone()],
        })
        .unwrap();

        assert!(data.is_dir(), "the data directory is emptied, not removed");
        assert!(!vera.exists(), "an owned directory is removed");
        assert!(keys.is_dir(), "a kept path survives");
        let store =
            RegolithStore::open_with_options(&data, RegolithStoreOptions::default()).unwrap();
        let txn = store.new_txn(true).await.unwrap();
        assert_eq!(txn.get(b"key").await.unwrap(), None);
    })
    .expect("run on a worker");
}

#[test]
fn wipe_refuses_an_owned_dir_holding_a_kept_path() {
    let root = tempfile::tempdir().unwrap();
    let keys = root.path().join("keys");
    std::fs::create_dir_all(&keys).unwrap();
    let error = storage::wipe(&WipeTarget {
        store: None,
        owned_dirs: vec![root.path().to_path_buf()],
        kept: vec![keys.clone()],
    })
    .unwrap_err();
    assert!(error.to_string().contains("refusing to wipe"), "{error}");
    assert!(keys.is_dir());
}

#[test]
fn wipe_refuses_a_kept_path_that_climbs_back_into_an_owned_dir() {
    let root = tempfile::tempdir().unwrap();
    let node = root.path().join("node");
    let vera = node.join("vera");
    let keys = vera.join("keys");
    std::fs::create_dir_all(&keys).unwrap();
    let error = storage::wipe(&WipeTarget {
        store: None,
        owned_dirs: vec![vera.clone()],
        kept: vec![node.join("../node/vera/keys")],
    })
    .unwrap_err();
    assert!(error.to_string().contains("refusing to wipe"), "{error}");
    assert!(keys.is_dir(), "the keyring survives");
}

#[test]
fn wipe_refuses_a_data_dir_that_climbs_back_into_an_owned_dir() {
    let root = tempfile::tempdir().unwrap();
    let node = root.path().join("node");
    let vera = node.join("vera");
    let error = storage::check_wipe(&WipeTarget {
        store: Some((
            node.join("./../node/vera/store"),
            RegolithStoreOptions::default(),
        )),
        owned_dirs: vec![vera],
        kept: Vec::new(),
    })
    .unwrap_err();
    assert!(error.to_string().contains("refusing to wipe"), "{error}");
}

#[test]
fn wipe_accepts_disjoint_paths_written_with_dots() {
    let root = tempfile::tempdir().unwrap();
    let node = root.path().join("node");
    storage::check_wipe(&WipeTarget {
        store: Some((node.join("./data"), RegolithStoreOptions::default())),
        owned_dirs: vec![node.join("vera"), node.join("x/../vera-worker")],
        kept: vec![node.join("../node/keys")],
    })
    .unwrap();
}

#[cfg(unix)]
#[test]
fn wipe_refuses_a_kept_path_that_climbs_through_a_symlink_into_an_owned_dir() {
    let root = tempfile::tempdir().unwrap();
    let vera = root.path().join("node/vera");
    let keys = vera.join("keys");
    std::fs::create_dir_all(vera.join("a/b")).unwrap();
    std::fs::create_dir_all(&keys).unwrap();
    std::os::unix::fs::symlink(vera.join("a/b"), vera.join("link")).unwrap();
    let error = storage::wipe(&WipeTarget {
        store: None,
        owned_dirs: vec![vera.clone()],
        kept: vec![vera.join("link/../../keys")],
    })
    .unwrap_err();
    assert!(error.to_string().contains("refusing to wipe"), "{error}");
    assert!(keys.is_dir(), "the keyring survives");
}

#[cfg(unix)]
#[test]
fn wipe_removes_an_owned_symlink_and_not_what_it_points_to() {
    let root = tempfile::tempdir().unwrap();
    let node = root.path().join("node");
    let keys = node.join("keys");
    let link = node.join("vera");
    std::fs::create_dir_all(&keys).unwrap();
    std::os::unix::fs::symlink(&node, &link).unwrap();
    storage::wipe(&WipeTarget {
        store: None,
        owned_dirs: vec![link.clone()],
        kept: vec![keys.clone()],
    })
    .unwrap();
    assert!(
        std::fs::symlink_metadata(&link).is_err(),
        "the link is removed"
    );
    assert!(keys.is_dir(), "the keyring the link pointed above survives");
}

#[cfg(unix)]
#[test]
fn wipe_refuses_a_kept_symlink_inside_an_owned_dir() {
    let root = tempfile::tempdir().unwrap();
    let vera = root.path().join("node/vera");
    let keyring = root.path().join("safe/keyring");
    let keys = vera.join("keys");
    std::fs::create_dir_all(&vera).unwrap();
    std::fs::create_dir_all(&keyring).unwrap();
    std::os::unix::fs::symlink(&keyring, &keys).unwrap();
    let error = storage::wipe(&WipeTarget {
        store: None,
        owned_dirs: vec![vera.clone()],
        kept: vec![keys.clone()],
    })
    .unwrap_err();
    assert!(error.to_string().contains("refusing to wipe"), "{error}");
    assert!(
        keys.is_dir(),
        "the configured keyring path still leads to it"
    );
}

#[cfg(unix)]
#[test]
fn wipe_refuses_a_kept_path_whose_link_chain_passes_through_an_owned_dir() {
    let root = tempfile::tempdir().unwrap();
    let vera = root.path().join("node/vera");
    let keyring = root.path().join("safe/keyring");
    let keys = root.path().join("node/keys");
    std::fs::create_dir_all(&vera).unwrap();
    std::fs::create_dir_all(&keyring).unwrap();
    std::os::unix::fs::symlink(&keyring, vera.join("hop")).unwrap();
    std::os::unix::fs::symlink(vera.join("hop"), &keys).unwrap();
    let error = storage::wipe(&WipeTarget {
        store: None,
        owned_dirs: vec![vera.clone()],
        kept: vec![keys.clone()],
    })
    .unwrap_err();
    assert!(error.to_string().contains("refusing to wipe"), "{error}");
    assert!(
        keys.is_dir(),
        "the configured keyring path still leads to it"
    );
}

#[cfg(unix)]
#[test]
fn wipe_accepts_a_kept_symlink_outside_every_owned_dir() {
    let root = tempfile::tempdir().unwrap();
    let vera = root.path().join("node/vera");
    let keyring = root.path().join("safe/keyring");
    let keys = root.path().join("node/keys");
    std::fs::create_dir_all(&vera).unwrap();
    std::fs::create_dir_all(&keyring).unwrap();
    std::os::unix::fs::symlink(&keyring, &keys).unwrap();
    storage::wipe(&WipeTarget {
        store: None,
        owned_dirs: vec![vera.clone()],
        kept: vec![keys.clone()],
    })
    .unwrap();
    assert!(!vera.exists(), "the owned directory is removed");
    assert!(keys.is_dir(), "the linked keyring survives");
}
