use std::sync::Arc;

use tracing::{info, warn};

use super::super::node::{Node, P2PTasks};
use super::P2PSetup;
use crate::config::Config;
use crate::error::{Error, Result};

impl Node {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn setup_iroh_p2p(
        store: Arc<storage::DynStore>,
        database: Arc<db::DB<storage::DynStore>>,
        event_bus: Arc<dyn events::Bus>,
        config: &Config,
        peer_keypair: Option<p2p::Keypair>,
        node_identity: Option<Arc<identity::RawIdentity>>,
        se_key: Option<[u8; 32]>,
        document_acp: Arc<dyn acp::DocumentACP>,
    ) -> Result<P2PSetup> {
        info!("Initializing P2P network (iroh)");

        #[cfg(feature = "iroh-relay-server")]
        let iroh_relay_server = Self::spawn_iroh_relay_server(config).await?;

        let mut peer_config = defra_p2p_adapter::IrohPeerConfig::new(
            p2p::iroh::IrohEndpointConfig {
                secret_key: Self::iroh_secret_key(peer_keypair.as_ref())?,
                node_identity,
                relay_mode: Self::iroh_relay_mode(config)?,
                discovery: Self::iroh_discovery(config)?,
                bind_port: config.net.iroh_bind_port,
                bind_addr: config.net.iroh_bind_addr,
                max_concurrent_multipath_paths: config.net.iroh_max_concurrent_multipath_paths,
                gossip_heal: p2p::iroh::GossipHealConfig::from_env(),
                allowlist: Self::iroh_allowlist(config),
            },
            document_acp,
        );
        peer_config.sync = Self::sync_config(config);
        peer_config.access_mode = Self::access_mode(config);
        peer_config.rebroadcast_on_merge = config.net.p2p_rebroadcast_on_merge;
        peer_config.max_merge_depth = config.datastore.max_merge_depth;
        peer_config.retry_schedule = config.retry_schedule()?;
        let peer = defra_p2p_adapter::IrohPeer::start(store, database, event_bus, peer_config)
            .await
            .map_err(Error::P2P)?;
        info!(
            "Iroh transport initialized, peer ID: {}",
            peer.local_peer_id
        );

        // A keyring-loaded SE key makes this node produce and verify SE
        // artifacts, and query replicators for encrypted collections. Identity
        // is None on both sides, so write-tags and query-tags agree.
        if let Some(key) = se_key {
            if let Err(e) =
                peer.replication
                    .broadcast_mutator
                    .set_se_options(db::merge::BroadcastSeOptions {
                        encryption_key: Some(zeroize::Zeroizing::new(key.to_vec())),
                        identity_pubkey: None,
                    })
            {
                warn!(error = %e, "failed to set searchable encryption options on broadcast mutator");
            }
            peer.replication
                .merge_handler_inner
                .set_se_enc_key(key.to_vec());
        }
        let se_transport =
            se_key.map(|key| peer.se_query_transport(db::merge::filled_se_key_handle(key, None)));

        let merge_handler_for_kms = Arc::clone(&peer.replication.merge_handler_inner);
        Ok(P2PSetup {
            host_handle: None,
            p2p_tasks: Some(P2PTasks::Iroh {
                peer: peer.shutdown.clone(),
                #[cfg(feature = "iroh-relay-server")]
                relay_server: iroh_relay_server,
            }),
            mutator: peer.replication.broadcast_mutator.clone(),
            http_adapter: Some(Arc::clone(&peer.ops)),
            txn_broadcaster: Some(Arc::clone(&peer.replication.txn_broadcaster)),
            wire_merge_acp: None,
            wire_doc_pusher_acp: None,
            kms_transport: Some(peer.kms_transport.clone() as Arc<dyn kms::KeyTransport>),
            wire_kms: Some(Box::new(move |kms| merge_handler_for_kms.set_kms(kms))),
            local_peer_id: peer.local_peer_id.clone(),
            se_transport,
            manage_hooks: Some(peer.manage.hooks.clone()),
            manage_controller: Some(Arc::clone(&peer.ops)),
            manage_correlator: Some(peer.manage.correlator.clone()),
            manage_query_correlator: Some(peer.manage.query_correlator.clone()),
            manage_requester: Some(Arc::clone(&peer.manage.requester)),
        })
    }

    /// Both transports are Ed25519, so the iroh endpoint reuses the peer key's
    /// seed and the node keeps one identity whichever transport it runs.
    fn iroh_secret_key(peer_keypair: Option<&p2p::Keypair>) -> Result<iroh_net::SecretKey> {
        let Some(kp) = peer_keypair else {
            return Ok(iroh_net::SecretKey::generate());
        };
        let ed25519 = kp
            .clone()
            .try_into_ed25519()
            .map_err(|_| Error::InvalidConfig("iroh transport requires Ed25519 key".into()))?;
        let seed: [u8; 32] =
            ed25519.secret().as_ref().try_into().map_err(|_| {
                Error::InvalidConfig("Ed25519 peer key seed must be 32 bytes".into())
            })?;
        Ok(iroh_net::SecretKey::from_bytes(&seed))
    }

    fn iroh_relay_mode(config: &Config) -> Result<p2p::iroh::IrohRelayModeConfig> {
        match config.net.iroh_relay_mode.as_deref() {
            Some("disabled") => Ok(p2p::iroh::IrohRelayModeConfig::Disabled),
            Some("default") => Ok(p2p::iroh::IrohRelayModeConfig::Default),
            Some("custom") => {
                let urls = Self::iroh_relay_urls(config);
                if urls.is_empty() {
                    Err(Error::InvalidConfig(
                        "iroh_relay_mode=custom requires at least one relay URL".into(),
                    ))
                } else {
                    Ok(p2p::iroh::IrohRelayModeConfig::Custom(urls))
                }
            }
            Some(other) => Err(Error::InvalidConfig(format!(
                "unsupported iroh_relay_mode '{}'",
                other
            ))),
            None => {
                let urls = Self::iroh_relay_urls(config);
                if urls.is_empty() {
                    Ok(p2p::iroh::IrohRelayModeConfig::Default)
                } else {
                    Ok(p2p::iroh::IrohRelayModeConfig::Custom(urls))
                }
            }
        }
    }

    fn iroh_relay_urls(config: &Config) -> Vec<String> {
        let mut urls: Vec<String> = config
            .net
            .iroh_relay_server
            .iter()
            .filter_map(|server| server.public_url.clone())
            .collect();
        urls.extend(config.net.iroh_relay_urls.iter().cloned());
        if let Some(url) = &config.net.iroh_relay_url {
            urls.push(url.clone());
        }
        urls
    }

    #[cfg(feature = "iroh-relay-server")]
    async fn spawn_iroh_relay_server(
        config: &Config,
    ) -> Result<Option<p2p::iroh::IrohRelayServer>> {
        let Some(server) = &config.net.iroh_relay_server else {
            return Ok(None);
        };
        p2p::iroh::IrohRelayServer::spawn(server.to_p2p())
            .await
            .map(Some)
            .map_err(Error::P2P)
    }

    /// Who may open an inbound connection.
    ///
    /// An empty `iroh_allowed_peers` keeps the behaviour every existing
    /// deployment has: accept everyone. Listing any id restricts inbound
    /// connections to the ids listed, and
    /// [`IrohTransport::allow_peer`](p2p::iroh::IrohTransport::allow_peer)
    /// can add more while the node runs. That runtime call is a no-op under
    /// `AcceptAll`, so without this setting the allowlist could not be
    /// turned on for this binary at all.
    fn iroh_allowlist(config: &Config) -> p2p::iroh::IrohAllowlistConfig {
        if config.net.iroh_allowed_peers.is_empty() {
            p2p::iroh::IrohAllowlistConfig::AcceptAll
        } else {
            p2p::iroh::IrohAllowlistConfig::Explicit(
                config.net.iroh_allowed_peers.iter().cloned().collect(),
            )
        }
    }

    fn iroh_discovery(config: &Config) -> Result<p2p::iroh::IrohDiscoveryConfig> {
        match (
            config.net.iroh_discovery,
            config.net.iroh_discovery_origin_domain.clone(),
            config.net.iroh_pkarr_relay_url.clone(),
        ) {
            (_, Some(origin_domain), Some(pkarr_relay_url)) => {
                Ok(p2p::iroh::IrohDiscoveryConfig::CustomDns {
                    origin_domain,
                    pkarr_relay_url,
                })
            }
            (_, Some(_), None) | (_, None, Some(_)) => Err(Error::InvalidConfig(
                "custom iroh discovery requires both iroh_discovery_origin_domain and iroh_pkarr_relay_url"
                    .into(),
            )),
            (false, None, None) => Ok(p2p::iroh::IrohDiscoveryConfig::Disabled),
            (true, None, None) => Ok(p2p::iroh::IrohDiscoveryConfig::N0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A node hosting a relay must also join it, or peers dialing through
    /// that relay could reach every endpoint except the one hosting it.
    #[test]
    fn hosted_relay_public_url_leads_the_relay_map() {
        let mut config = Config::default();
        config.net.iroh_relay_urls = vec!["https://other.example.com".to_string()];
        config.net.iroh_relay_server = Some(
            serde_yaml::from_str(
                "http_bind_addr: 0.0.0.0:80\npublic_url: https://relay.example.com\n",
            )
            .unwrap(),
        );

        assert_eq!(
            Node::iroh_relay_mode(&config).unwrap(),
            p2p::iroh::IrohRelayModeConfig::Custom(vec![
                "https://relay.example.com".to_string(),
                "https://other.example.com".to_string(),
            ])
        );
    }

    /// The setting is what turns the allowlist on for this binary.
    ///
    /// `IrohTransport::allow_peer` is a no-op under `AcceptAll`, so a
    /// deployment that cannot express `Explicit` here cannot restrict
    /// inbound connections at all, whatever it does at runtime. Both
    /// directions are asserted: the default stays open, matching every
    /// existing deployment, and a listed id closes it to exactly that set.
    #[test]
    fn listed_peers_switch_the_endpoint_to_an_explicit_allowlist() {
        let mut config = Config::default();
        assert_eq!(
            Node::iroh_allowlist(&config),
            p2p::iroh::IrohAllowlistConfig::AcceptAll,
            "an unset allowlist must not change what an existing node accepts"
        );

        config.net.iroh_allowed_peers = vec!["peer-one".to_string(), "peer-two".to_string()];
        assert_eq!(
            Node::iroh_allowlist(&config),
            p2p::iroh::IrohAllowlistConfig::Explicit(
                ["peer-one".to_string(), "peer-two".to_string()]
                    .into_iter()
                    .collect()
            )
        );
    }

    #[test]
    fn iroh_secret_key_is_the_peer_key() {
        let keypair = p2p::Keypair::generate_ed25519();
        let libp2p_public = keypair.public().try_into_ed25519().unwrap().to_bytes();

        let iroh_key = Node::iroh_secret_key(Some(&keypair)).unwrap();

        assert_eq!(iroh_key.public().as_bytes(), &libp2p_public);
    }
}
