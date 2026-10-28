//! A node's transport runs on the network runtime its config names, or on one
//! the node starts itself when it names none; a node without networking needs none.
//! The node's database runs its network work on the same runtime: a push
//! that waits on a pool worker, which has no reactor, crosses to it. Each
//! build runs on a plain thread with no runtime entered, so a transport that
//! looked for one ambiently would panic here.

#![cfg(any(feature = "libp2p", feature = "p2p"))]

use std::sync::Arc;

#[path = "support/outcome.rs"]
mod outcome;

use defra_node::{NodeConfig, TransportConfig};
use outcome::Result;

fn memory_store() -> Result<storage::RegolithStore> {
    Ok(storage::RegolithStore::open_with_options(
        "memory",
        storage::RegolithStoreOptions::memory(),
    )?)
}

async fn build_and_stop(config: NodeConfig) -> Result<()> {
    // The build fails when the transport does not start.
    let node = defra_node::build_with_store(Arc::new(memory_store()?), config).await?;
    node.shutdown().await;
    Ok(())
}

/// The network runtime the node's database runs its network work on.
async fn database_edge(config: NodeConfig) -> Result<Option<edge::Edge>> {
    let node = defra_node::build_with_store(Arc::new(memory_store()?), config).await?;
    let edge = node.database().network().edge().cloned();
    node.shutdown().await;
    Ok(edge)
}

#[cfg(feature = "libp2p")]
fn libp2p() -> TransportConfig {
    TransportConfig::Libp2p(defra_node::Libp2pConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
        ..Default::default()
    })
}

#[cfg(feature = "p2p")]
fn iroh() -> TransportConfig {
    TransportConfig::Iroh(defra_node::P2PConfig {
        bind_addr: Some(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        port: Some(0),
        relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
        discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
        max_concurrent_multipath_paths: None,
        secret_key_path: None,
        allowlist: Default::default(),
        peer_key: defra_node::PeerKeySource::Store,
        ..Default::default()
    })
}

#[cfg(feature = "libp2p")]
#[test]
fn a_transport_without_a_network_runtime_runs_on_the_process_runtime() -> Result<()> {
    workers::block_on(build_and_stop(NodeConfig {
        transport: libp2p(),
        ..Default::default()
    }))
    .unwrap()
}

#[cfg(feature = "libp2p")]
#[test]
fn a_transport_runs_on_the_network_runtime_it_is_given() -> Result<()> {
    workers::block_on(build_and_stop(NodeConfig {
        transport: libp2p(),
        edge: Some(edge::testing::edge()),
        ..Default::default()
    }))
    .unwrap()
}

#[cfg(feature = "p2p")]
#[test]
fn an_iroh_transport_runs_on_the_network_runtime_it_is_given() -> Result<()> {
    workers::block_on(build_and_stop(NodeConfig {
        transport: iroh(),
        edge: Some(edge::testing::edge()),
        ..Default::default()
    }))
    .unwrap()
}

#[cfg(feature = "libp2p")]
#[test]
fn a_node_without_a_network_runtime_starts_one_for_its_database_and_stops_it() -> Result<()> {
    workers::block_on(async {
        let edge = database_edge(NodeConfig {
            transport: libp2p(),
            ..Default::default()
        })
        .await?
        .expect("a transport needs a network runtime");
        assert!(edge.is_closed(), "the node stops a runtime it started");
        Ok(())
    })
    .unwrap()
}

#[cfg(feature = "libp2p")]
#[test]
fn a_node_gives_its_database_a_scope_of_the_network_runtime_it_is_given() -> Result<()> {
    workers::block_on(async {
        let given = edge::testing::edge();
        let edge = database_edge(NodeConfig {
            transport: libp2p(),
            edge: Some(given.clone()),
            ..Default::default()
        })
        .await?
        .expect("a given runtime is used");
        assert_ne!(edge, given, "the node works through a scope of its own");
        assert!(edge.is_closed(), "shutdown drains the node's scope");
        assert!(!given.is_closed(), "and leaves the runtime it was given");
        Ok(())
    })
    .unwrap()
}

/// On the shared test runtime, not a plain thread: an iroh node's shutdown
/// waits on the runtime it is called from.
#[cfg(feature = "p2p")]
#[test]
fn an_iroh_node_without_a_network_runtime_starts_one_for_its_database_and_stops_it() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let edge = database_edge(NodeConfig {
                transport: iroh(),
                ..Default::default()
            })
            .await?
            .expect("a transport needs a network runtime");
            assert!(edge.is_closed(), "the node stops a runtime it started");
            Ok(())
        })
        .unwrap()
}

#[test]
fn a_node_without_networking_gives_its_database_no_network_runtime() -> Result<()> {
    workers::block_on(async {
        let edge = database_edge(NodeConfig {
            transport: TransportConfig::None,
            ..Default::default()
        })
        .await?;
        assert_eq!(edge, None);
        Ok(())
    })
    .unwrap()
}
