//! `net.transport: vinc`: the vinc peer, direct connections only. It reads
//! the iroh bind and allowlist settings (`net.iroh_bind_addr`,
//! `net.iroh_bind_port`, `net.iroh_allowed_peers`) until vinc has keys of its
//! own.

use std::sync::Arc;

use tracing::{info, warn};

use super::super::node::{Node, P2PTasks};
use super::P2PSetup;
use crate::config::Config;
use crate::error::{Error, Result};

impl Node {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn setup_vinc_p2p(
        store: Arc<storage::DynStore>,
        database: Arc<db::DB<storage::DynStore>>,
        event_bus: Arc<dyn events::Bus>,
        config: &Config,
        peer_keypair: Option<p2p::Keypair>,
        node_identity: Option<Arc<identity::RawIdentity>>,
        se_key: Option<[u8; 32]>,
        document_acp: Arc<dyn acp::DocumentACP>,
        edge: &edge::Edge,
    ) -> Result<P2PSetup> {
        info!("Initializing P2P network (vinc)");
        let bind = std::net::SocketAddr::new(
            config
                .net
                .iroh_bind_addr
                .unwrap_or(std::net::IpAddr::from([0, 0, 0, 0])),
            config.net.iroh_bind_port.unwrap_or(0),
        );
        let mut endpoint =
            p2p::vinc::VincEndpointConfig::new(Self::vinc_seed(peer_keypair.as_ref())?, bind);
        endpoint.node_identity = node_identity;
        endpoint.allowlist = if config.net.iroh_allowed_peers.is_empty() {
            p2p::vinc::VincAllowlistConfig::AcceptAll
        } else {
            p2p::vinc::VincAllowlistConfig::Explicit(
                config.net.iroh_allowed_peers.iter().cloned().collect(),
            )
        };
        let mut peer_config =
            defra_p2p_adapter::VincPeerConfig::new(endpoint, document_acp, edge.clone());
        peer_config.sync = Self::sync_config(config);
        peer_config.access_mode = Self::access_mode(config);
        peer_config.max_merge_depth = config.datastore.max_merge_depth;
        peer_config.retry_schedule = config.retry_schedule()?;
        let peer = defra_p2p_adapter::VincPeer::start(store, database, event_bus, peer_config)
            .await
            .map_err(Error::P2P)?;
        info!(
            "vinc transport initialized, peer ID: {}",
            peer.local_peer_id
        );

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
            p2p_tasks: Some(P2PTasks::Vinc {
                peer: peer.shutdown.clone(),
            }),
            mutator: peer.replication.broadcast_mutator.clone(),
            http_adapter: Some(Arc::clone(&peer.ops)),
            txn_broadcaster: Some(Arc::clone(&peer.replication.txn_broadcaster)),
            wire_merge_acp: None,
            wire_doc_pusher_acp: None,
            kms_transport: Some(Arc::clone(&peer.kms_transport)),
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

    /// The peer key's Ed25519 seed, so the node keeps the id it has on libp2p
    /// and iroh; a fresh one when the node has no peer key.
    fn vinc_seed(peer_keypair: Option<&p2p::Keypair>) -> Result<zeroize::Zeroizing<[u8; 32]>> {
        let Some(kp) = peer_keypair else {
            let key = crypto::generate_ed25519()
                .map_err(|e| Error::InvalidConfig(format!("vinc peer key: {e}")))?;
            return Ok(zeroize::Zeroizing::new(key.underlying().to_bytes()));
        };
        let ed25519 = kp
            .clone()
            .try_into_ed25519()
            .map_err(|_| Error::InvalidConfig("vinc transport requires an Ed25519 key".into()))?;
        let seed: [u8; 32] =
            ed25519.secret().as_ref().try_into().map_err(|_| {
                Error::InvalidConfig("Ed25519 peer key seed must be 32 bytes".into())
            })?;
        Ok(zeroize::Zeroizing::new(seed))
    }
}
