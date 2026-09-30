#![cfg(feature = "libp2p")]

use std::sync::Arc;

use anyhow::{anyhow, Result};
use embedded::NodeBuilder;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
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
            .with_peer_keyring(keyring)
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
        identities.push(identity);
        node.shutdown().await;
    }
    assert_eq!(identities[0], identities[1]);
    Ok(())
}
