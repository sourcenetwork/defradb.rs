//! A node's transport runs on the network runtime its config names, or on the
//! process's own when it names none; a node without networking needs none.
//! Each build runs on a plain thread with no runtime entered, so a transport
//! that looked for one ambiently would panic here.

#![cfg(any(feature = "libp2p", feature = "iroh"))]

use std::sync::Arc;

#[path = "support/outcome.rs"]
mod outcome;

use embedded::{EmbeddedNodeConfig, TransportConfig};
use outcome::Result;

fn memory_store() -> Result<storage::RegolithStore> {
    Ok(storage::RegolithStore::open_with_options(
        "memory",
        storage::RegolithStoreOptions::memory(),
    )?)
}

async fn build_and_stop(config: EmbeddedNodeConfig) -> Result<()> {
    // The build fails when the transport does not start.
    let node = embedded::build_with_store(Arc::new(memory_store()?), config).await?;
    node.shutdown().await;
    Ok(())
}

#[cfg(feature = "libp2p")]
fn libp2p() -> TransportConfig {
    TransportConfig::Libp2p(embedded::Libp2pConfig {
        listen_addr: "/ip4/127.0.0.1/tcp/0".to_string(),
    })
}

#[cfg(feature = "iroh")]
fn iroh() -> TransportConfig {
    TransportConfig::Iroh(embedded::IrohConfig {
        bind_addr: Some(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        bind_port: Some(0),
        relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
        discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
        max_concurrent_multipath_paths: None,
        secret_key_path: None,
        allowlist: Default::default(),
    })
}

#[cfg(feature = "libp2p")]
#[test]
fn a_transport_without_a_network_runtime_runs_on_the_process_runtime() -> Result<()> {
    workers::block_on(build_and_stop(EmbeddedNodeConfig {
        transport: libp2p(),
        ..Default::default()
    }))
    .unwrap()
}

#[cfg(feature = "libp2p")]
#[test]
fn a_transport_runs_on_the_network_runtime_it_is_given() -> Result<()> {
    workers::block_on(build_and_stop(EmbeddedNodeConfig {
        transport: libp2p(),
        edge: Some(edge::testing::edge()),
        ..Default::default()
    }))
    .unwrap()
}

#[cfg(feature = "iroh")]
#[test]
fn an_iroh_transport_runs_on_the_network_runtime_it_is_given() -> Result<()> {
    workers::block_on(build_and_stop(EmbeddedNodeConfig {
        transport: iroh(),
        edge: Some(edge::testing::edge()),
        ..Default::default()
    }))
    .unwrap()
}
