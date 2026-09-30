use std::sync::Arc;

use super::*;
use keyring::Keyring;

fn store() -> Peerstore<storage::RegolithStore> {
    Peerstore::new(Arc::new(storage::RegolithStore::in_memory().unwrap()))
}

#[tokio::test]
async fn keyring_identity_survives_reopen_without_plaintext_peerstore_key() {
    let dir = tempfile::tempdir().unwrap();
    let peerstore = store();
    let first = {
        let keyring = crate::PeerKeyring::new(Arc::new(
            keyring::FileKeyring::open(dir.path(), b"test-password").unwrap(),
        ));
        load_with_keyring(&peerstore, None, Some(&keyring))
            .await
            .unwrap()
    };
    assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
    let reopened = keyring::FileKeyring::open(dir.path(), b"test-password").unwrap();
    assert_eq!(reopened.get(keyring::PEER_KEY).unwrap().len(), 64);
    let keyring = crate::PeerKeyring::new(Arc::new(reopened));
    assert_eq!(
        *first,
        *load_with_keyring(&peerstore, None, Some(&keyring))
            .await
            .unwrap()
    );
    assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
}

#[tokio::test]
async fn existing_plaintext_identity_is_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let peerstore = store();
    peerstore.set_local_peer_key(&[7; 32]).await.unwrap();
    let backend = Arc::new(keyring::FileKeyring::open(dir.path(), b"test-password").unwrap());
    let keyring = crate::PeerKeyring::new(backend.clone());
    let error = load_with_keyring(&peerstore, None, Some(&keyring))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("explicit keyring migration"));
    assert_eq!(
        peerstore
            .get_local_peer_key()
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        &[7; 32]
    );
    assert!(matches!(
        backend.get(keyring::PEER_KEY),
        Err(keyring::Error::NotFound(_))
    ));
}

#[tokio::test]
async fn corrupt_keyring_key_is_not_regenerated() {
    let dir = tempfile::tempdir().unwrap();
    let peerstore = store();
    let backend = Arc::new(keyring::FileKeyring::open(dir.path(), b"test-password").unwrap());
    backend.set(keyring::PEER_KEY, b"corrupt").unwrap();
    let keyring = crate::PeerKeyring::new(backend.clone());
    assert!(load_with_keyring(&peerstore, None, Some(&keyring))
        .await
        .is_err());
    assert_eq!(
        backend.get(keyring::PEER_KEY).unwrap().as_slice(),
        b"corrupt"
    );
    assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
}

struct FailingKeyring {
    fail_read: bool,
}

#[tokio::test]
async fn legacy_key_files_are_preserved_and_require_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("iroh.key");
    tokio::fs::write(&path, [7; 32]).await.unwrap();
    let peerstore = store();
    let keyring = crate::PeerKeyring::new(Arc::new(FailingKeyring { fail_read: true }));
    let error = load_with_keyring(&peerstore, Some(&path), Some(&keyring))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("explicit keyring migration"));
    assert_eq!(tokio::fs::read(path).await.unwrap(), [7; 32]);
    assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
}

#[tokio::test]
async fn legacy_libp2p_key_requires_migration_even_without_that_transport() {
    let peerstore = store();
    peerstore
        .create_replicator(LEGACY_LIBP2P_KEY_ID, b"legacy-key")
        .await
        .unwrap();
    let keyring = crate::PeerKeyring::new(Arc::new(FailingKeyring { fail_read: true }));
    let error = load_with_keyring(&peerstore, None, Some(&keyring))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("explicit keyring migration"));
    assert_eq!(
        peerstore
            .get_replicator(LEGACY_LIBP2P_KEY_ID)
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        b"legacy-key"
    );
    assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
}

impl Keyring for FailingKeyring {
    fn get(&self, name: &str) -> keyring::Result<Zeroizing<Vec<u8>>> {
        if self.fail_read {
            Err(keyring::Error::SystemKeyring("locked".into()))
        } else {
            Err(keyring::Error::NotFound(name.into()))
        }
    }

    fn set(&self, _: &str, _: &[u8]) -> keyring::Result<()> {
        assert!(
            !self.fail_read,
            "read failure must not create a replacement key"
        );
        Err(keyring::Error::SystemKeyring("unwritable".into()))
    }

    fn delete(&self, _: &str) -> keyring::Result<()> {
        unreachable!()
    }
    fn list(&self) -> keyring::Result<Vec<String>> {
        unreachable!()
    }
}

#[tokio::test]
async fn keyring_errors_never_fall_back_to_plaintext_storage() {
    for fail_read in [true, false] {
        let peerstore = store();
        let keyring = crate::PeerKeyring::new(Arc::new(FailingKeyring { fail_read }));
        assert!(load_with_keyring(&peerstore, None, Some(&keyring))
            .await
            .is_err());
        assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
    }
}
