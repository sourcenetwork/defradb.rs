//! A node's transport runs on the network runtime its config names, or on the
//! process's own when it names none; a node without networking needs none.

#![cfg(feature = "libp2p")]

use std::sync::Arc;

#[path = "support/outcome.rs"]
mod outcome;

use embedded::{EmbeddedNodeConfig, Libp2pConfig, TransportConfig};
use outcome::Result;

fn memory_store() -> Result<storage::RegolithStore> {
    Ok(storage::RegolithStore::open_with_options(
        "memory",
        storage::RegolithStoreOptions::memory(),
    )?)
}

fn libp2p() -> TransportConfig {
    TransportConfig::Libp2p(Libp2pConfig {
        listen_addr: "/ip4/127.0.0.1/tcp/0".to_string(),
    })
}

#[test]
fn a_transport_without_a_network_runtime_runs_on_the_process_runtime() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let config = EmbeddedNodeConfig {
                transport: libp2p(),
                ..Default::default()
            };

            // The build fails when the transport does not start.
            let node = embedded::build_with_store(Arc::new(memory_store()?), config).await?;
            node.shutdown().await;
            Ok(())
        })
        .unwrap()
}

#[test]
fn a_transport_runs_on_the_network_runtime_it_is_given() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let config = EmbeddedNodeConfig {
                transport: libp2p(),
                edge: Some(edge::testing::edge()),
                ..Default::default()
            };

            let node = embedded::build_with_store(Arc::new(memory_store()?), config).await?;

            node.shutdown().await;
            Ok(())
        })
        .unwrap()
}
