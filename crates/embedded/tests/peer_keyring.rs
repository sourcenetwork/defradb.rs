#![cfg(any(feature = "libp2p", feature = "iroh"))]

use std::sync::Arc;

use anyhow::{anyhow, Result};
use embedded::NodeBuilder;
use keyring::Keyring;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(feature = "libp2p")]
async fn application_keyring_preserves_peer_identity_across_node_restarts() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut identities = Vec::new();
    for _ in 0..2 {
        let keyring = Arc::new(keyring::FileKeyring::open(
            dir.path().join("keys"),
            b"test-password",
        )?);
        let node = NodeBuilder::default()
            .data_path(dir.path().join("node"))
            .with_peer_keyring(keyring.clone())
            .with_libp2p("/ip4/127.0.0.1/tcp/0")
            .build()
            .await?;
        let identity = node
            .p2p()
            .expect("libp2p node")
            .ops()
            .local_peer_id()
            .await
            .map_err(|error| anyhow!(error))?;
        let key = keyring.get(keyring::PEER_KEY)?;
        let mut seed: [u8; 32] = key[..32].try_into()?;
        let expected = libp2p::identity::Keypair::ed25519_from_bytes(&mut seed)?;
        assert_eq!(
            identity.as_str(),
            expected.public().to_peer_id().to_string()
        );
        identities.push(identity);
        node.shutdown().await;
    }
    assert_eq!(identities[0], identities[1]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(feature = "iroh")]
async fn iroh_uses_the_application_keyring_identity_after_restart() -> Result<()> {
    use crypto::{Key, PrivateKey};
    use embedded::IrohConfig;

    let dir = tempfile::tempdir()?;
    let keyring = Arc::new(keyring::FileKeyring::open(
        dir.path().join("keys"),
        b"test-password",
    )?);
    let key = crypto::generate_ed25519()?;
    keyring.set(keyring::PEER_KEY, key.raw())?;
    let expected = hex::encode(key.public_key().raw());
    for _ in 0..2 {
        let node = NodeBuilder::default()
            .data_path(dir.path().join("node"))
            .with_peer_keyring(keyring.clone())
            .with_iroh(IrohConfig {
                bind_addr: Some(std::net::Ipv4Addr::LOCALHOST.into()),
                bind_port: Some(0),
                relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
                discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
                max_concurrent_multipath_paths: None,
                secret_key_path: None,
                allowlist: Default::default(),
            })
            .build()
            .await?;
        let identity = node
            .p2p()
            .expect("iroh node")
            .ops()
            .local_peer_id()
            .await
            .map_err(|error| anyhow!(error))?;
        assert_eq!(identity, expected);
        node.shutdown().await;
    }
    Ok(())
}
