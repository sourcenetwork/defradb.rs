//! The node's peer key: one Ed25519 seed that libp2p and iroh both use, so a
//! node keeps one identity whichever transport it runs.
//!
//! An application keyring keeps new peer keys outside the database. Without
//! one, the existing peerstore-backed identity remains supported.

use std::path::Path;

use crate::error::{Error, Result, SeedLength};
use storage::stores::Peerstore;
use zeroize::Zeroizing;

/// Replicator slot where libp2p kept its keypair before the shared peer key.
pub(crate) const LEGACY_LIBP2P_KEY_ID: &str = "__local_p2p_identity__";

pub(crate) type Seed = Zeroizing<[u8; 32]>;

pub(crate) async fn load_with_keyring<S: storage::corekv::Store>(
    peerstore: &Peerstore<S>,
    legacy_iroh_key: Option<&Path>,
    keyring: Option<&crate::PeerKeyring>,
) -> Result<Seed> {
    let Some(keyring) = keyring else {
        return load_or_create(peerstore, legacy_iroh_key).await;
    };
    if peerstore.get_local_peer_key().await?.is_some() {
        return Err(Error::PlaintextPeerKey);
    }
    if legacy_iroh_seed(legacy_iroh_key).await?.is_some() {
        return Err(Error::PlaintextIrohKeyFile(
            legacy_iroh_key.expect("existing key file").to_path_buf(),
        ));
    }
    if peerstore
        .get_replicator(LEGACY_LIBP2P_KEY_ID)
        .await?
        .is_some()
    {
        return Err(Error::PlaintextLegacyKey(LEGACY_LIBP2P_KEY_ID));
    }

    let expected = peerstore.get_local_peer_keyring_identity().await?;
    use crypto::{Key, PrivateKey};
    let key = match keyring.0.get(keyring::PEER_KEY) {
        Ok(bytes) => {
            crypto::Ed25519PrivateKey::from_bytes(&bytes).map_err(Error::KeyringKeyCorrupt)?
        }
        Err(keyring::Error::NotFound(_)) => {
            if expected.is_some() {
                return Err(Error::KeyringKeyMissing);
            }
            let key = crypto::generate_ed25519().map_err(Error::PeerKeyGenerate)?;
            keyring
                .0
                .set(keyring::PEER_KEY, key.raw())
                .map_err(Error::KeyringPersist)?;
            key
        }
        Err(error) => return Err(Error::KeyringRead(error)),
    };
    let public_key = key.public_key();
    match expected {
        Some(expected) if expected.as_ref() != public_key.raw() => {
            return Err(Error::KeyringIdentityMismatch);
        }
        None => {
            peerstore
                .set_local_peer_keyring_identity(public_key.raw())
                .await?
        }
        Some(_) => {}
    }
    Ok(seed_from_slice(&key.raw()[..32])?)
}

/// Load the node's peer key seed, creating it on first start.
///
/// A node that predates the shared key keeps the identity its transport
/// already had: the iroh key file, else libp2p's peerstore entry, is imported
/// once. `legacy_iroh_key` is only ever read.
///
/// The legacy libp2p entry is removed only when it was the key imported. A
/// database that ran both transports has two identities and only one can win,
/// but the one that loses is key material, and deleting it is unrecoverable.
/// It stays where it is; `node_recovery` knows to pass over the slot.
pub(crate) async fn load_or_create<S: storage::corekv::Store>(
    peerstore: &Peerstore<S>,
    legacy_iroh_key: Option<&Path>,
) -> Result<Seed> {
    if peerstore.get_local_peer_keyring_identity().await?.is_some() {
        return Err(Error::PeerKeyringRequired);
    }
    if let Some(bytes) = peerstore
        .get_local_peer_key()
        .await
        .map_err(Error::PeerKeyRead)?
    {
        return seed_from_slice(&bytes).map_err(Error::StoredPeerKeyCorrupt);
    }

    let (seed, from_legacy_libp2p) = match legacy_iroh_seed(legacy_iroh_key).await? {
        Some(seed) => (seed, false),
        None => match legacy_libp2p_seed(peerstore).await? {
            Some(seed) => (seed, true),
            None => (generate()?, false),
        },
    };
    peerstore
        .set_local_peer_key(seed.as_slice())
        .await
        .map_err(Error::PeerKeyStore)?;
    if from_legacy_libp2p {
        remove_legacy_libp2p_key(peerstore).await?;
    }
    Ok(seed)
}

async fn legacy_iroh_seed(path: Option<&Path>) -> Result<Option<Seed>> {
    let Some(path) = path else {
        return Ok(None);
    };
    match std::fs::read(path) {
        Ok(bytes) => seed_from_slice(&Zeroizing::new(bytes))
            .map_err(|source| Error::IrohKeyFileCorrupt {
                path: path.to_path_buf(),
                source,
            })
            .map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::IrohKeyFileRead {
            path: path.to_path_buf(),
            source: error,
        }),
    }
}

#[cfg(feature = "libp2p")]
async fn legacy_libp2p_seed<S: storage::corekv::Store>(
    peerstore: &Peerstore<S>,
) -> Result<Option<Seed>> {
    let Some(bytes) = peerstore
        .get_replicator(LEGACY_LIBP2P_KEY_ID)
        .await
        .map_err(Error::LegacyKeyRead)?
    else {
        return Ok(None);
    };
    let keypair = libp2p::identity::Keypair::from_protobuf_encoding(&bytes)
        .map_err(Error::LegacyKeyCorrupt)?;
    let ed25519 = keypair
        .try_into_ed25519()
        .map_err(|_| Error::LegacyKeyNotEd25519)?;
    Ok(Some(seed_from_slice(ed25519.secret().as_ref())?))
}

#[cfg(not(feature = "libp2p"))]
async fn legacy_libp2p_seed<S: storage::corekv::Store>(
    _peerstore: &Peerstore<S>,
) -> Result<Option<Seed>> {
    Ok(None)
}

#[cfg(feature = "libp2p")]
async fn remove_legacy_libp2p_key<S: storage::corekv::Store>(
    peerstore: &Peerstore<S>,
) -> Result<()> {
    peerstore
        .delete_replicator(LEGACY_LIBP2P_KEY_ID)
        .await
        .map_err(Error::LegacyKeyRemove)
}

#[cfg(not(feature = "libp2p"))]
async fn remove_legacy_libp2p_key<S: storage::corekv::Store>(
    _peerstore: &Peerstore<S>,
) -> Result<()> {
    Ok(())
}

fn generate() -> Result<Seed> {
    use crypto::Key;

    let key = crypto::generate_ed25519().map_err(Error::PeerKeyGenerate)?;
    Ok(seed_from_slice(&key.raw()[..32])?)
}

fn seed_from_slice(bytes: &[u8]) -> Result<Seed, SeedLength> {
    let seed: [u8; 32] = bytes.try_into().map_err(|_| SeedLength(bytes.len()))?;
    Ok(Zeroizing::new(seed))
}

#[cfg(test)]
#[path = "../tests/unit/peer_keyring.rs"]
mod keyring_tests;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn peerstore() -> Peerstore<storage::RegolithStore> {
        Peerstore::new(Arc::new(storage::RegolithStore::in_memory().unwrap()))
    }

    #[test]
    fn created_key_is_reused() {
        workers::block_on(async {
            let peerstore = peerstore();

            let first = load_or_create(&peerstore, None).await.unwrap();
            let second = load_or_create(&peerstore, None).await.unwrap();

            assert_eq!(*first, *second);
        })
        .unwrap()
    }

    #[test]
    fn iroh_key_file_is_imported_once_and_left_untouched() {
        workers::block_on(async {
            let peerstore = peerstore();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("node.iroh.key");
            std::fs::write(&path, [7u8; 32]).unwrap();

            let imported = load_or_create(&peerstore, Some(&path)).await.unwrap();
            assert_eq!(*imported, [7u8; 32]);
            assert_eq!(std::fs::read(&path).unwrap(), [7u8; 32]);

            std::fs::remove_file(&path).unwrap();
            let reloaded = load_or_create(&peerstore, Some(&path)).await.unwrap();
            assert_eq!(*reloaded, [7u8; 32]);
        })
        .unwrap()
    }

    #[test]
    fn missing_iroh_key_file_is_not_created() {
        workers::block_on(async {
            let peerstore = peerstore();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("node.iroh.key");

            load_or_create(&peerstore, Some(&path)).await.unwrap();

            assert!(!path.exists());
        })
        .unwrap()
    }

    #[test]
    fn corrupt_stored_key_fails_instead_of_regenerating() {
        workers::block_on(async {
            let peerstore = peerstore();
            peerstore.set_local_peer_key(b"short").await.unwrap();

            assert!(load_or_create(&peerstore, None).await.is_err());
        })
        .unwrap()
    }

    #[test]
    fn corrupt_iroh_key_file_fails() {
        workers::block_on(async {
            let peerstore = peerstore();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("node.iroh.key");
            std::fs::write(&path, b"short").unwrap();

            assert!(load_or_create(&peerstore, Some(&path)).await.is_err());
        })
        .unwrap()
    }

    #[cfg(feature = "libp2p")]
    fn legacy_libp2p_key_bytes(seed: [u8; 32]) -> Vec<u8> {
        libp2p::identity::Keypair::ed25519_from_bytes(seed)
            .unwrap()
            .to_protobuf_encoding()
            .unwrap()
    }

    #[cfg(feature = "libp2p")]
    #[test]
    fn legacy_libp2p_key_is_imported_and_removed() {
        workers::block_on(async {
            let peerstore = peerstore();
            peerstore
                .create_replicator(LEGACY_LIBP2P_KEY_ID, &legacy_libp2p_key_bytes([9u8; 32]))
                .await
                .unwrap();

            let imported = load_or_create(&peerstore, None).await.unwrap();

            assert_eq!(*imported, [9u8; 32]);
            assert!(peerstore
                .get_replicator(LEGACY_LIBP2P_KEY_ID)
                .await
                .unwrap()
                .is_none());
        })
        .unwrap()
    }

    /// Only one of the two identities can win, but the loser is key material:
    /// it is left where it is rather than deleted along with the import.
    #[cfg(feature = "libp2p")]
    #[test]
    fn iroh_key_file_wins_without_discarding_the_libp2p_identity() {
        workers::block_on(async {
            let peerstore = peerstore();
            peerstore
                .create_replicator(LEGACY_LIBP2P_KEY_ID, &legacy_libp2p_key_bytes([9u8; 32]))
                .await
                .unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("node.iroh.key");
            std::fs::write(&path, [7u8; 32]).unwrap();

            let imported = load_or_create(&peerstore, Some(&path)).await.unwrap();

            assert_eq!(*imported, [7u8; 32]);
            assert_eq!(
                peerstore
                    .get_replicator(LEGACY_LIBP2P_KEY_ID)
                    .await
                    .unwrap()
                    .as_deref(),
                Some(legacy_libp2p_key_bytes([9u8; 32]).as_slice()),
                "the identity that lost must still be recoverable"
            );
        })
        .unwrap()
    }

    #[cfg(feature = "libp2p")]
    #[test]
    fn corrupt_legacy_libp2p_key_fails() {
        workers::block_on(async {
            let peerstore = peerstore();
            peerstore
                .create_replicator(LEGACY_LIBP2P_KEY_ID, b"not a keypair")
                .await
                .unwrap();

            assert!(load_or_create(&peerstore, None).await.is_err());
        })
        .unwrap()
    }
}
