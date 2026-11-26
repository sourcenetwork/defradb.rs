#![cfg(feature = "p2p")]

use std::sync::Arc;

#[path = "support/outcome.rs"]
mod outcome;

use defra_node::NodeBuilder;
use keyring::Keyring;
use outcome::Result;

#[test]
#[cfg(feature = "p2p")]
fn pinc_uses_the_application_keyring_identity_after_restart() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            use crypto::{Key, PrivateKey};
            use defra_node::PincConfig;

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
                    .with_pinc(PincConfig {
                        bind: std::net::SocketAddr::new(
                            std::net::IpAddr::from(std::net::Ipv4Addr::LOCALHOST),
                            0,
                        ),
                        secret_key_path: None,
                        allowlist: Default::default(),
                        peer_key: defra_node::PeerKeySource::Store,
                        ..Default::default()
                    })
                    .build()
                    .await?;
                let identity = node
                    .p2p_system()
                    .expect("pinc node")
                    .ops()
                    .local_peer_id()
                    .await?;
                assert_eq!(identity, expected);
                node.shutdown().await.unwrap();
            }
            Ok(())
        })
        .unwrap()
}
