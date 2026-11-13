use std::sync::Arc;

use super::*;
use keyring::Keyring;

#[test]
fn keyring_marker_rejects_missing_and_replaced_backends() {
    workers::block_on(async {
        use crypto::Key;

        let peerstore = store();
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(keyring::FileKeyring::open(dir.path(), b"password").unwrap());
        let keyring = crate::PeerKeyring::new(backend.clone());
        let seed = load_with_keyring(&peerstore, None, Some(&keyring), None)
            .await
            .unwrap();
        assert!(load_with_keyring(&peerstore, None, None, None)
            .await
            .unwrap_err()
            .to_string()
            .contains("requires its application-owned peer keyring"));
        assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
        let original = backend.get(keyring::PEER_KEY).unwrap();
        backend.delete(keyring::PEER_KEY).unwrap();
        assert!(load_with_keyring(&peerstore, None, Some(&keyring), None)
            .await
            .is_err());
        assert!(matches!(
            backend.get(keyring::PEER_KEY),
            Err(keyring::Error::NotFound(_))
        ));
        backend
            .set(keyring::PEER_KEY, crypto::generate_ed25519().unwrap().raw())
            .unwrap();
        assert!(load_with_keyring(&peerstore, None, Some(&keyring), None)
            .await
            .unwrap_err()
            .to_string()
            .contains("does not match"));
        backend.set(keyring::PEER_KEY, &original).unwrap();
        assert_eq!(
            *load_with_keyring(&peerstore, None, Some(&keyring), None)
                .await
                .unwrap(),
            *seed
        );
    })
    .unwrap()
}

fn store() -> Peerstore<storage::RegolithStore> {
    Peerstore::new(Arc::new(storage::RegolithStore::in_memory().unwrap()))
}

#[test]
fn keyring_identity_survives_reopen_without_plaintext_peerstore_key() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let peerstore = store();
        let first = {
            let keyring = crate::PeerKeyring::new(Arc::new(
                keyring::FileKeyring::open(dir.path(), b"test-password").unwrap(),
            ));
            load_with_keyring(&peerstore, None, Some(&keyring), None)
                .await
                .unwrap()
        };
        assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
        let reopened = keyring::FileKeyring::open(dir.path(), b"test-password").unwrap();
        assert_eq!(reopened.get(keyring::PEER_KEY).unwrap().len(), 64);
        let keyring = crate::PeerKeyring::new(Arc::new(reopened));
        assert_eq!(
            *first,
            *load_with_keyring(&peerstore, None, Some(&keyring), None)
                .await
                .unwrap()
        );
        assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
    })
    .unwrap()
}

#[test]
fn existing_plaintext_identity_is_not_replaced() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let peerstore = store();
        peerstore.set_local_peer_key(&[7; 32]).await.unwrap();
        let backend = Arc::new(keyring::FileKeyring::open(dir.path(), b"test-password").unwrap());
        let keyring = crate::PeerKeyring::new(backend.clone());
        let error = load_with_keyring(&peerstore, None, Some(&keyring), None)
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
    })
    .unwrap()
}

#[test]
fn corrupt_keyring_key_is_not_regenerated() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let peerstore = store();
        let backend = Arc::new(keyring::FileKeyring::open(dir.path(), b"test-password").unwrap());
        backend.set(keyring::PEER_KEY, b"corrupt").unwrap();
        let keyring = crate::PeerKeyring::new(backend.clone());
        assert!(load_with_keyring(&peerstore, None, Some(&keyring), None)
            .await
            .is_err());
        assert_eq!(
            backend.get(keyring::PEER_KEY).unwrap().as_slice(),
            b"corrupt"
        );
        assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
    })
    .unwrap()
}

struct FailingKeyring {
    fail_read: bool,
}

#[test]
fn legacy_key_files_are_preserved_and_require_migration() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("iroh.key");
        std::fs::write(&path, [7; 32]).unwrap();
        let peerstore = store();
        let keyring = crate::PeerKeyring::new(Arc::new(FailingKeyring { fail_read: true }));
        let error = load_with_keyring(&peerstore, Some(&path), Some(&keyring), None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("explicit keyring migration"));
        assert_eq!(std::fs::read(path).unwrap(), [7; 32]);
        assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
    })
    .unwrap()
}

#[test]
fn legacy_libp2p_key_requires_migration_even_without_that_transport() {
    workers::block_on(async {
        let peerstore = store();
        peerstore
            .create_replicator(LEGACY_LIBP2P_KEY_ID, b"legacy-key")
            .await
            .unwrap();
        let keyring = crate::PeerKeyring::new(Arc::new(FailingKeyring { fail_read: true }));
        let error = load_with_keyring(&peerstore, None, Some(&keyring), None)
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
    })
    .unwrap()
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

#[test]
fn keyring_errors_never_fall_back_to_plaintext_storage() {
    workers::block_on(async {
        for fail_read in [true, false] {
            let peerstore = store();
            let keyring = crate::PeerKeyring::new(Arc::new(FailingKeyring { fail_read }));
            assert!(load_with_keyring(&peerstore, None, Some(&keyring), None)
                .await
                .is_err());
            assert!(peerstore.get_local_peer_key().await.unwrap().is_none());
        }
    })
    .unwrap()
}

/// `proofs/tla/PeerKeyLoad.tla` INV_StartKeyStarts and INV_PlainKeyStable: a
/// start key starts a store that holds a plaintext key, is used as given,
/// and leaves the store as it was, so the next plain start uses the store's
/// own key again.
#[test]
fn a_start_key_is_used_as_given_and_leaves_a_plain_store_as_it_was() {
    workers::block_on(async {
        use crypto::Key;

        let peerstore = store();
        let plain = load_with_keyring(&peerstore, None, None, None)
            .await
            .unwrap();
        let start_key = crate::StartPeerKey::from_ed25519_private_key(
            crypto::generate_ed25519().unwrap().raw(),
        )
        .unwrap();

        let used = load_with_keyring(&peerstore, None, None, Some(&start_key))
            .await
            .unwrap();
        assert_eq!(*used, **start_key.seed(), "the start key is used as given");
        assert!(peerstore
            .get_local_peer_keyring_identity()
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            *load_with_keyring(&peerstore, None, None, None)
                .await
                .unwrap(),
            *plain,
            "a plain start after it uses the store's own key"
        );
    })
    .unwrap()
}

/// `proofs/tla/PeerKeyLoad.tla` INV_StartKeyLeavesNoMark: a start key starts
/// a store that records a keyring identity, without changing it, so the
/// keyring keeps working and a plain start is refused just as before.
#[test]
fn a_start_key_starts_a_keyring_store_without_changing_it() {
    workers::block_on(async {
        use crypto::Key;

        let peerstore = store();
        let dir = tempfile::tempdir().unwrap();
        let keyring = crate::PeerKeyring::new(Arc::new(
            keyring::FileKeyring::open(dir.path(), b"password").unwrap(),
        ));
        let pinned = load_with_keyring(&peerstore, None, Some(&keyring), None)
            .await
            .unwrap();
        let identity = peerstore.get_local_peer_keyring_identity().await.unwrap();
        let start_key = crate::StartPeerKey::from_ed25519_private_key(
            crypto::generate_ed25519().unwrap().raw(),
        )
        .unwrap();

        assert!(load_with_keyring(&peerstore, None, None, Some(&start_key))
            .await
            .is_ok());
        assert_eq!(
            peerstore.get_local_peer_keyring_identity().await.unwrap(),
            identity,
            "the recorded identity is unchanged"
        );
        assert_eq!(
            *load_with_keyring(&peerstore, None, Some(&keyring), None)
                .await
                .unwrap(),
            *pinned
        );
        assert!(load_with_keyring(&peerstore, None, None, None)
            .await
            .is_err());
    })
    .unwrap()
}

#[test]
fn a_start_key_must_be_an_ed25519_private_key() {
    assert!(crate::StartPeerKey::from_ed25519_private_key(&[1, 2, 3]).is_err());
}
