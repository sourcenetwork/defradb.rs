//! The node's peer key: one Ed25519 seed that libp2p and iroh both use, so a
//! node keeps one identity whichever transport it runs.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use storage::stores::Peerstore;
use zeroize::Zeroizing;

/// Replicator slot where libp2p kept its keypair before the shared peer key.
#[cfg(feature = "libp2p")]
const LEGACY_LIBP2P_KEY_ID: &str = "__local_p2p_identity__";

pub(crate) type Seed = Zeroizing<[u8; 32]>;

/// Load the node's peer key seed, creating it on first start.
///
/// A node that predates the shared key keeps the identity its transport
/// already had: the iroh key file, else libp2p's peerstore entry, is imported
/// once. `legacy_iroh_key` is only ever read.
pub(crate) async fn load_or_create<S: storage::corekv::Store>(
    peerstore: &Peerstore<S>,
    legacy_iroh_key: Option<&Path>,
) -> Result<Seed> {
    if let Some(bytes) = peerstore
        .get_local_peer_key()
        .await
        .context("failed to read peer key")?
    {
        return seed_from_slice(&bytes).context("stored peer key is corrupt");
    }

    let seed = match legacy_iroh_seed(legacy_iroh_key).await? {
        Some(seed) => seed,
        None => match legacy_libp2p_seed(peerstore).await? {
            Some(seed) => seed,
            None => generate()?,
        },
    };
    peerstore
        .set_local_peer_key(seed.as_slice())
        .await
        .context("failed to store peer key")?;
    remove_legacy_libp2p_key(peerstore).await?;
    Ok(seed)
}

async fn legacy_iroh_seed(path: Option<&Path>) -> Result<Option<Seed>> {
    let Some(path) = path else {
        return Ok(None);
    };
    match tokio::fs::read(path).await {
        Ok(bytes) => seed_from_slice(&Zeroizing::new(bytes))
            .with_context(|| format!("iroh key file '{}' is corrupt", path.display()))
            .map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read iroh key file '{}'", path.display()))
        }
    }
}

#[cfg(feature = "libp2p")]
async fn legacy_libp2p_seed<S: storage::corekv::Store>(
    peerstore: &Peerstore<S>,
) -> Result<Option<Seed>> {
    let Some(bytes) = peerstore
        .get_replicator(LEGACY_LIBP2P_KEY_ID)
        .await
        .context("failed to read legacy libp2p peer key")?
    else {
        return Ok(None);
    };
    let keypair = libp2p::identity::Keypair::from_protobuf_encoding(&bytes)
        .map_err(|error| anyhow!("legacy libp2p peer key is corrupt: {error}"))?;
    let ed25519 = keypair
        .try_into_ed25519()
        .map_err(|_| anyhow!("legacy libp2p peer key is not Ed25519"))?;
    seed_from_slice(ed25519.secret().as_ref()).map(Some)
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
        .context("failed to remove legacy libp2p peer key")
}

#[cfg(not(feature = "libp2p"))]
async fn remove_legacy_libp2p_key<S: storage::corekv::Store>(
    _peerstore: &Peerstore<S>,
) -> Result<()> {
    Ok(())
}

fn generate() -> Result<Seed> {
    use crypto::Key;

    let key = crypto::generate_ed25519()
        .map_err(|error| anyhow!("failed to generate peer key: {error}"))?;
    seed_from_slice(&key.raw()[..32])
}

fn seed_from_slice(bytes: &[u8]) -> Result<Seed> {
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("expected a 32-byte Ed25519 seed, got {} bytes", bytes.len()))?;
    Ok(Zeroizing::new(seed))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn peerstore() -> Peerstore<storage::RegolithStore> {
        Peerstore::new(Arc::new(storage::RegolithStore::in_memory().unwrap()))
    }

    #[tokio::test]
    async fn created_key_is_reused() {
        let peerstore = peerstore();

        let first = load_or_create(&peerstore, None).await.unwrap();
        let second = load_or_create(&peerstore, None).await.unwrap();

        assert_eq!(*first, *second);
    }

    #[tokio::test]
    async fn iroh_key_file_is_imported_once_and_left_untouched() {
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
    }

    #[tokio::test]
    async fn missing_iroh_key_file_is_not_created() {
        let peerstore = peerstore();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("node.iroh.key");

        load_or_create(&peerstore, Some(&path)).await.unwrap();

        assert!(!path.exists());
    }

    #[tokio::test]
    async fn corrupt_stored_key_fails_instead_of_regenerating() {
        let peerstore = peerstore();
        peerstore.set_local_peer_key(b"short").await.unwrap();

        assert!(load_or_create(&peerstore, None).await.is_err());
    }

    #[tokio::test]
    async fn corrupt_iroh_key_file_fails() {
        let peerstore = peerstore();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("node.iroh.key");
        std::fs::write(&path, b"short").unwrap();

        assert!(load_or_create(&peerstore, Some(&path)).await.is_err());
    }

    #[cfg(feature = "libp2p")]
    fn legacy_libp2p_key_bytes(seed: [u8; 32]) -> Vec<u8> {
        libp2p::identity::Keypair::ed25519_from_bytes(seed)
            .unwrap()
            .to_protobuf_encoding()
            .unwrap()
    }

    #[cfg(feature = "libp2p")]
    #[tokio::test]
    async fn legacy_libp2p_key_is_imported_and_removed() {
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
    }

    #[cfg(feature = "libp2p")]
    #[tokio::test]
    async fn iroh_key_file_wins_over_legacy_libp2p_key() {
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
    }

    #[cfg(feature = "libp2p")]
    #[tokio::test]
    async fn corrupt_legacy_libp2p_key_fails() {
        let peerstore = peerstore();
        peerstore
            .create_replicator(LEGACY_LIBP2P_KEY_ID, b"not a keypair")
            .await
            .unwrap();

        assert!(load_or_create(&peerstore, None).await.is_err());
    }
}
