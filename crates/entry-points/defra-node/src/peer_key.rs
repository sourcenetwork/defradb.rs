//! The node's peer key: the Ed25519 seed its pinc peer id comes from.
//!
//! An application keyring keeps new peer keys outside the database. Without
//! one, the existing peerstore-backed identity remains supported.

use std::path::Path;

use crate::error::{Error, Result, SeedLength};
use storage::stores::Peerstore;
use zeroize::Zeroizing;

/// Replicator slot where a libp2p node kept its keypair; a keyring cannot
/// take it over.
pub(crate) const LEGACY_LIBP2P_KEY_ID: &str = "__local_p2p_identity__";

pub(crate) type Seed = Zeroizing<[u8; 32]>;

/// The node's peer key seed for this start: the caller's start key as given,
/// else the keyring's, else the store's (`proofs/tla/PeerKeyLoad.tla`).
pub(crate) async fn load_with_keyring<S>(
    peerstore: &Peerstore<S>,
    key_file: Option<&Path>,
    keyring: Option<&crate::PeerKeyring>,
    start_key: Option<&crate::StartPeerKey>,
) -> Result<Seed>
where
    S: storage::corekv::Store,
{
    if let Some(start_key) = start_key {
        return Ok(start_key.seed().clone());
    }
    let Some(keyring) = keyring else {
        return load_or_create(peerstore, key_file).await;
    };
    if peerstore.get_local_peer_key().await?.is_some() {
        return Err(Error::PlaintextPeerKey);
    }
    if key_file_seed(key_file).await?.is_some() {
        return Err(Error::PlaintextPeerKeyFile(
            key_file.expect("existing key file").to_path_buf(),
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
/// A node that predates the stored key keeps the identity it had: its key
/// file, when one is named and exists, is imported once. `key_file` is only
/// ever read.
pub(crate) async fn load_or_create<S>(
    peerstore: &Peerstore<S>,
    key_file: Option<&Path>,
) -> Result<Seed>
where
    S: storage::corekv::Store,
{
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

    let seed = match key_file_seed(key_file).await? {
        Some(seed) => seed,
        None => generate()?,
    };
    peerstore
        .set_local_peer_key(seed.as_slice())
        .await
        .map_err(Error::PeerKeyStore)?;
    Ok(seed)
}

async fn key_file_seed(path: Option<&Path>) -> Result<Option<Seed>> {
    let Some(path) = path else {
        return Ok(None);
    };
    match std::fs::read(path) {
        Ok(bytes) => seed_from_slice(&Zeroizing::new(bytes))
            .map_err(|source| Error::PeerKeyFileCorrupt {
                path: path.to_path_buf(),
                source,
            })
            .map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::PeerKeyFileRead {
            path: path.to_path_buf(),
            source: error,
        }),
    }
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
    fn key_file_is_imported_once_and_left_untouched() {
        workers::block_on(async {
            let peerstore = peerstore();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("node.key");
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
    fn missing_key_file_is_not_created() {
        workers::block_on(async {
            let peerstore = peerstore();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("node.key");

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
    fn corrupt_key_file_fails() {
        workers::block_on(async {
            let peerstore = peerstore();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("node.key");
            std::fs::write(&path, b"short").unwrap();

            assert!(load_or_create(&peerstore, Some(&path)).await.is_err());
        })
        .unwrap()
    }
}
