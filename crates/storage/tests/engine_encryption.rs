//! At-rest keys reach the engine before open, including read-only opens.

use storage::backends::RegolithTxn;
use storage::corekv::{Reader, Store, Writer};
use storage::{RegolithStore, RegolithStoreOptions};

const KEY: [u8; 32] = [0x71; 32];

#[test]
fn encrypted_engine_reopens_and_requires_the_same_key() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let options = RegolithStoreOptions::default().with_encryption_key(KEY);
        let store = RegolithStore::open_with_options(dir.path(), options.clone()).unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        txn.set(b"private-key", b"private-value").await.unwrap();
        txn.commit().await.unwrap();
        store.close().await.unwrap();
        drop(store);

        fn check_files(path: &std::path::Path) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    check_files(&path);
                } else {
                    let bytes = std::fs::read(&path).unwrap();
                    for secret in [b"private-key".as_slice(), b"private-value".as_slice()] {
                        assert!(
                            !bytes.windows(secret.len()).any(|window| window == secret),
                            "plaintext in {}",
                            path.display()
                        );
                    }
                }
            }
        }
        check_files(dir.path());
        assert!(RegolithStore::open(dir.path()).is_err());
        assert!(RegolithTxn::open_read_only(dir.path(), RegolithStoreOptions::default()).is_err());
        assert!(RegolithStore::open_with_options(
            dir.path(),
            RegolithStoreOptions::default().with_encryption_key([0x72; 32]),
        )
        .is_err());

        let txn = RegolithTxn::open_read_only(dir.path(), options.clone()).unwrap();
        assert_eq!(
            txn.get(b"private-key").await.unwrap().as_deref(),
            Some(&b"private-value"[..])
        );
        drop(txn);

        let store = RegolithStore::open_with_options(dir.path(), options).unwrap();
        let txn = store.new_txn(true).await.unwrap();
        assert_eq!(
            txn.get(b"private-key").await.unwrap().as_deref(),
            Some(&b"private-value"[..])
        );
    })
    .unwrap()
}

#[test]
fn encryption_keeps_plaintext_values_inside_the_engine() {
    workers::block_on(async {
        let options = RegolithStoreOptions::memory().with_encryption_key(KEY);
        let store = RegolithStore::open_with_options("encrypted-memory", options).unwrap();
        let mut txn = store.new_txn(false).await.unwrap();
        txn.set(b"counter", &1024u64.to_be_bytes()).await.unwrap();
        txn.commit().await.unwrap();
        let txn = store.new_txn(true).await.unwrap();
        assert_eq!(
            txn.get(b"counter").await.unwrap().as_deref(),
            Some(&1024u64.to_be_bytes()[..])
        );
    })
    .unwrap()
}
