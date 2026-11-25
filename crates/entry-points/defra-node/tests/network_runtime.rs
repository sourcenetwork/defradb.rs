//! A node's transport runs on the network runtime its config names, or on one
//! the node starts itself when it names none; a node without networking needs none.
//! The node's database runs its network work on the same runtime: a push
//! that waits on a pool worker, which has no reactor, crosses to it. Each
//! build runs on a plain thread with no runtime entered, so a transport that
//! looked for one ambiently would panic here.

#![cfg(feature = "p2p")]

#[path = "support/outcome.rs"]
mod outcome;

use defra_node::{NodeBuilder, NodeConfig, TransportConfig};
use outcome::Result;

async fn build_and_stop(config: NodeConfig) -> Result<()> {
    // The build fails when the transport does not start.
    let node = NodeBuilder::default().with_config(config).build().await?;
    node.shutdown().await.unwrap();
    Ok(())
}

/// The network runtime the node's database runs its network work on.
async fn database_edge(config: NodeConfig) -> Result<Option<edge::Edge>> {
    let node = NodeBuilder::default().with_config(config).build().await?;
    let edge = node
        .database()
        .network()
        .ok()
        .map(|network| network.edge().clone());
    node.shutdown().await.unwrap();
    Ok(edge)
}

#[cfg(feature = "p2p")]
fn pinc() -> TransportConfig {
    TransportConfig::Pinc(defra_node::PincConfig {
        bind: std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 0),
        secret_key_path: None,
        allowlist: Default::default(),
        peer_key: defra_node::PeerKeySource::Store,
        ..Default::default()
    })
}

#[cfg(feature = "p2p")]
#[test]
fn a_pinc_transport_runs_on_the_network_runtime_it_is_given() -> Result<()> {
    workers::block_on(build_and_stop(NodeConfig {
        transport: pinc(),
        edge: Some(edge::testing::edge()),
        ..Default::default()
    }))
    .unwrap()
}

/// On the shared test runtime, not a plain thread: a pinc node's shutdown
/// waits on the runtime it is called from.
#[cfg(feature = "p2p")]
#[test]
fn a_pinc_node_without_a_network_runtime_starts_one_for_its_database_and_stops_it() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let edge = database_edge(NodeConfig {
                transport: pinc(),
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
