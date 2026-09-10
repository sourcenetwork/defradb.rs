#![cfg(all(feature = "libp2p", feature = "iroh"))]

use std::net::{IpAddr, Ipv4Addr};

use anyhow::{anyhow, Context, Result};
use embedded::{EmbeddedNode, EmbeddedStore, IrohConfig, NodeBuilder};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn libp2p_and_iroh_share_the_node_key() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let data_path = dir.path().join("node");

    let libp2p_node = NodeBuilder::default()
        .data_path(data_path.clone())
        .with_libp2p("/ip4/127.0.0.1/tcp/0")
        .build()
        .await?;
    let libp2p_id = local_peer_id(&libp2p_node).await?;
    libp2p_node.shutdown().await;
    drop(libp2p_node);

    let iroh_node = NodeBuilder::default()
        .data_path(data_path)
        .with_iroh(iroh_config())
        .build()
        .await?;
    let iroh_id = local_peer_id(&iroh_node).await?;
    iroh_node.shutdown().await;

    assert_eq!(iroh_id, hex::encode(libp2p_ed25519_public(&libp2p_id)?));
    Ok(())
}

async fn local_peer_id(node: &EmbeddedNode<EmbeddedStore>) -> Result<String> {
    node.p2p()
        .context("node has no p2p system")?
        .ops()
        .local_peer_id()
        .await
        .map_err(|error| anyhow!(error))
}

fn libp2p_ed25519_public(peer_id: &str) -> Result<[u8; 32]> {
    let peer_id: libp2p::PeerId = peer_id.parse()?;
    let public = libp2p::identity::PublicKey::try_decode_protobuf(peer_id.as_ref().digest())?;
    Ok(public.try_into_ed25519()?.to_bytes())
}

fn iroh_config() -> IrohConfig {
    IrohConfig {
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        bind_port: Some(0),
        relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
        discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
        max_concurrent_multipath_paths: None,
        secret_key_path: None,
    }
}
