//! Regression tests for the Node::shutdown API (#813).
//!
//! Verifies:
//! 1. `shutdown()` can be called and returns
//! 2. After `shutdown()`, `is_shutdown()` reports true
//! 3. `shutdown()` is idempotent — repeated calls are safe no-ops
//! 4. After `shutdown()`, the database reports `is_closed() == true`
//! 5. Concurrent `shutdown()` callers block until teardown completes

#[path = "support/outcome.rs"]
mod outcome;

#[cfg(feature = "libp2p")]
use defra_node::Libp2pConfig;
#[cfg(feature = "p2p")]
use defra_node::P2PConfig;
#[cfg(any(feature = "libp2p", feature = "p2p"))]
use defra_node::TransportConfig;
use defra_node::{NodeBuilder, NodeConfig};
use outcome::Result;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn shutdown_reports_closed_database() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let node = NodeBuilder::default().build().await?;
            assert!(!node.is_shutdown(), "fresh node should not report shutdown");
            assert!(
                !node.database().is_closed(),
                "fresh database should not report closed"
            );

            node.shutdown().await.unwrap();

            assert!(
                node.is_shutdown(),
                "after shutdown(), is_shutdown() should return true"
            );
            assert!(
                node.database().is_closed(),
                "after shutdown(), database should report closed"
            );

            Ok(())
        })
        .unwrap()
}

#[test]
fn shutdown_is_idempotent() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let node = NodeBuilder::default().build().await?;

            // First call does real work.
            node.shutdown().await.unwrap();
            assert!(node.is_shutdown());
            assert!(node.database().is_closed());

            // Second and third calls must be safe no-ops.
            node.shutdown().await.unwrap();
            node.shutdown().await.unwrap();
            assert!(node.is_shutdown());
            assert!(node.database().is_closed());

            Ok(())
        })
        .unwrap()
}

#[test]
fn shutdown_concurrent_calls_are_safe() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let node = Arc::new(NodeBuilder::default().build().await?);

            // Fire several concurrent shutdown() calls. They should all return
            // without panicking, and the node should end up fully shut down.
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let n = node.clone();
                    edge::testing::edge().spawn(async move { n.shutdown().await })
                })
                .collect();

            for h in handles {
                h.await.expect("shutdown task should not panic").unwrap();
            }

            assert!(node.is_shutdown());
            assert!(node.database().is_closed());

            Ok(())
        })
        .unwrap()
}
#[cfg(feature = "libp2p")]
#[test]
fn p2p_shutdown_releases_persistent_store() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("data.regolith");
            let config = NodeConfig {
                edge: Some(edge::testing::edge()),
                transport: TransportConfig::Libp2p(Libp2pConfig {
                    listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
                    ..Default::default()
                }),
                ..Default::default()
            };
            let node = NodeBuilder::default()
                .with_config(config)
                .data_path(&path)
                .build()
                .await?;

            node.shutdown().await.unwrap();
            drop(node);

            let reopened = storage::RegolithStore::open(&path)?;
            storage::Store::close(&reopened).await?;

            Ok(())
        })
        .unwrap()
}

#[cfg(feature = "p2p")]
#[test]
fn iroh_shutdown_releases_persistent_store() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("data.regolith");
            let config = NodeConfig {
                edge: Some(edge::testing::edge()),
                transport: TransportConfig::Iroh(P2PConfig {
                    peer_key: defra_node::PeerKeySource::Store,
                    bind_addr: Some(std::net::Ipv4Addr::LOCALHOST.into()),
                    port: Some(0),
                    relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
                    discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let node = NodeBuilder::default()
                .with_config(config)
                .data_path(&path)
                .build()
                .await?;

            node.shutdown().await.unwrap();
            drop(node);

            let reopened = storage::RegolithStore::open(&path)?;
            storage::Store::close(&reopened).await?;

            Ok(())
        })
        .unwrap()
}

#[cfg(feature = "libp2p")]
#[test]
fn failed_p2p_setup_releases_persistent_store() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("data.regolith");
            let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
            let port = listener.local_addr()?.port();
            let config = NodeConfig {
                edge: Some(edge::testing::edge()),
                transport: TransportConfig::Libp2p(Libp2pConfig {
                    listen_addrs: vec![format!("/ip4/127.0.0.1/tcp/{port}")],
                    ..Default::default()
                }),
                ..Default::default()
            };

            let result = NodeBuilder::default()
                .with_config(config)
                .data_path(&path)
                .build()
                .await;
            assert!(
                result.is_err(),
                "P2P setup should fail when the port is busy"
            );

            let reopened = storage::RegolithStore::open(&path)?;
            storage::Store::close(&reopened).await?;

            Ok(())
        })
        .unwrap()
}

/// NAC asked for by a node with no identity to own it is refused before the
/// host starts (MC_NodeStartup_Red_LateNacCheck), so the store is free.
#[cfg(feature = "libp2p")]
#[test]
fn unownable_node_acp_releases_persistent_store() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("data.regolith");
            let config = NodeConfig {
                edge: Some(edge::testing::edge()),
                transport: TransportConfig::Libp2p(Libp2pConfig {
                    listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
                    ..Default::default()
                }),
                node_acp_enabled: true,
                ..Default::default()
            };

            let result = NodeBuilder::default()
                .with_config(config)
                .data_path(&path)
                .build()
                .await;
            assert!(
                matches!(result, Err(defra_node::Error::NodeAcpNeedsIdentity)),
                "NAC without a node identity must be refused"
            );

            let reopened = storage::RegolithStore::open(&path)?;
            storage::Store::close(&reopened).await?;

            Ok(())
        })
        .unwrap()
}

#[test]
fn concurrent_shutdown_callers_wait_for_teardown_completion() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let node = Arc::new(NodeBuilder::default().build().await?);

            let callers: Vec<_> = (0..4)
                .map(|_| {
                    let node = node.clone();
                    edge::testing::edge().spawn(async move {
                        node.shutdown().await.unwrap();
                        assert!(
                            node.database().is_closed(),
                            "shutdown() returned before the database closed"
                        );
                    })
                })
                .collect();

            edge::testing::edge()
                .timeout(Duration::from_secs(5), async {
                    for caller in callers {
                        caller.await.expect("a shutdown task should not panic");
                    }
                })
                .await
                .expect("shutdown tasks did not finish");

            Ok(())
        })
        .unwrap()
}
