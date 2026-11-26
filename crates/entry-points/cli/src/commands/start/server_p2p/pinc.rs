//! The pinc peer: direct connections on `net.pinc_bind_addr` (loopback
//! unless set, as Go) and `net.pinc_bind_port`, the extra addresses
//! `net.pinc_public_addrs` lists, inbound admission from
//! `net.pinc_allowed_peers`, topics through the hubs `net.pinc_hubs` names,
//! and a dial of each of `net.peers` at start.

use std::sync::Arc;

use tracing::{info, warn};

use super::super::node::{Node, P2PTasks};
use super::P2PSetup;
use crate::config::Config;
use crate::error::{Error, Result};

impl Node {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn setup_pinc_p2p(
        store: Arc<storage::DynStore>,
        database: Arc<db::DB<storage::DynStore>>,
        event_bus: Arc<dyn events::Bus>,
        config: &Config,
        peer_seed: Option<zeroize::Zeroizing<[u8; 32]>>,
        node_identity: Option<Arc<identity::RawIdentity>>,
        se_key: Option<[u8; 32]>,
        document_acp: Arc<dyn acp::DocumentACP>,
        edge: &edge::Edge,
    ) -> Result<P2PSetup> {
        info!("Initializing P2P network (pinc)");
        let bind = config.net.pinc_bind();
        if !bind.ip().is_loopback() && config.net.pinc_allowed_peers.is_empty() {
            warn!(%bind, "the pinc endpoint is reachable from other hosts and net.pinc_allowed_peers is empty: any peer may connect");
        }
        let seed = match peer_seed {
            Some(seed) => seed,
            None => defra_pinc::generate_seed().map_err(Error::P2P)?,
        };
        let mut endpoint = defra_pinc::PincEndpointConfig::new(seed, bind);
        endpoint.public_addrs = config.net.pinc_public_addrs.clone();
        endpoint.node_identity = node_identity;
        endpoint.allowlist = if config.net.pinc_allowed_peers.is_empty() {
            defra_pinc::PincAllowlistConfig::AcceptAll
        } else {
            defra_pinc::PincAllowlistConfig::Explicit(
                config.net.pinc_allowed_peers.iter().cloned().collect(),
            )
        };
        endpoint.hubs = config.net.pinc_hubs.clone();
        if endpoint.hubs.is_empty() {
            warn!("net.pinc_hubs names no hub: topic broadcasts and the KMS key exchange reach no peer");
        }
        let mut peer_config =
            defra_p2p_adapter::PincPeerConfig::new(endpoint, document_acp, edge.clone());
        peer_config.sync = Self::sync_config(config);
        peer_config.access_mode = Self::access_mode(config);
        peer_config.max_merge_depth = config.datastore.max_merge_depth;
        peer_config.retry_schedule = config.retry_schedule()?;
        peer_config.rebroadcast_on_merge = config.net.p2p_rebroadcast_on_merge;
        let peer = defra_p2p_adapter::PincPeer::start(store, database, event_bus, peer_config)
            .await
            .map_err(Error::P2P)?;
        info!(
            "pinc transport initialized, peer ID: {}",
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

        // The bootstrap peers are dialed in the background, so one that does
        // not answer does not hold up the start.
        let bootstrap_peers = config.net.peers.clone();
        let bootstrap_ops = Arc::clone(&peer.ops);
        let bootstrap_edge = edge.clone();
        let connect_bootstrap: Box<dyn FnOnce() + Send> = Box::new(move || {
            for addr in bootstrap_peers {
                let ops = Arc::clone(&bootstrap_ops);
                bootstrap_edge
                    .spawn_service(async move {
                        if let Err(error) = ops.connect_peer(&addr).await {
                            warn!(peer = %addr, %error, "bootstrap peer did not connect");
                        }
                    })
                    .detach();
            }
        });
        Ok(P2PSetup {
            p2p_tasks: Some(P2PTasks {
                peer: peer.shutdown.clone(),
            }),
            mutator: peer.replication.broadcast_mutator.clone(),
            http_adapter: Some(Arc::clone(&peer.ops)),
            txn_broadcaster: Some(Arc::clone(&peer.replication.txn_broadcaster)),
            wire_merge_acp: None,
            wire_doc_pusher_acp: None,
            kms_transport: Some(peer.kms_transport.clone() as Arc<dyn kms::KeyTransport>),
            wire_kms: Some(Box::new(peer.kms_binder())),
            local_peer_id: peer.local_peer_id.clone(),
            se_transport,
            manage_hooks: Some(peer.manage.hooks.clone()),
            manage_controller: Some(Arc::clone(&peer.ops)),
            manage_correlator: Some(peer.manage.correlator.clone()),
            manage_query_correlator: Some(peer.manage.query_correlator.clone()),
            manage_requester: Some(Arc::clone(&peer.manage.requester)),
            connect_bootstrap: Some(connect_bootstrap),
        })
    }
}
