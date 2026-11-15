//! ACP and NAC initialization helpers for Node startup.

use std::sync::Arc;

use tracing::info;

use super::node::Node;
#[cfg(feature = "vera")]
use crate::config::AcpDocumentType;
use crate::config::Config;
use crate::error::{Error, Result};
use identity::Did;

pub(super) struct DocumentAcpSetup {
    pub(super) document_acp: Arc<dyn acp::DocumentACP>,
    /// The node's DAC policies, installed on the database.
    pub(super) dac_policies: Arc<db::DacPolicies>,
    /// The Vera node started for document ACP; shutdown stops it.
    #[cfg(feature = "embedded-vera")]
    pub(super) embedded_vera: Option<vera_embedded::HostedNode>,
}

impl Node {
    #[cfg_attr(not(feature = "vera"), allow(unused_variables))]
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn setup_document_acp(
        config: &Config,
        identity_key_bytes: Option<&[u8]>,
        _acp_store: Arc<dyn acp::AcpStore>,
        store: Arc<storage::DynStore>,
        zanzibar_store: Arc<dyn acp::ZanzibarStore>,
        event_bus: Arc<dyn events::Bus>,
        edge: &edge::Edge,
    ) -> Result<DocumentAcpSetup> {
        #[cfg(feature = "vera")]
        if config.acp.document_type == AcpDocumentType::Vera {
            if config.acp.vera_address.is_empty() {
                return Err(Error::InvalidConfig(
                    "vera_address required when document_type is vera".into(),
                ));
            }

            let signer_key_bytes = identity_key_bytes.ok_or_else(|| {
                Error::InvalidConfig("node identity required for Vera ACP (use --identity)".into())
            })?;

            let tuning = vera::AcpTuning {
                request_timeout: std::time::Duration::from_secs(config.acp.request_timeout),
                circuit_breaker_threshold: config.acp.circuit_breaker_threshold,
                circuit_breaker_reset_timeout: std::time::Duration::from_secs(
                    config.acp.circuit_breaker_reset_timeout,
                ),
                cache_ttl: std::time::Duration::from_secs(config.acp.cache_ttl),
                receipt_timeout: std::time::Duration::from_secs(config.acp.receipt_timeout),
            };

            info!(
                request_timeout_s = config.acp.request_timeout,
                circuit_breaker_threshold = config.acp.circuit_breaker_threshold,
                circuit_breaker_reset_timeout_s = config.acp.circuit_breaker_reset_timeout,
                cache_ttl_s = config.acp.cache_ttl,
                receipt_timeout_s = config.acp.receipt_timeout,
                "Resolved ACP tuning (Vera)"
            );

            let provider = Arc::new(
                vera::CosmosProvider::new_with_grpc(
                    config.acp.vera_address.clone(),
                    if config.acp.vera_grpc_address.is_empty() {
                        config.acp.vera_address.clone()
                    } else {
                        config.acp.vera_grpc_address.clone()
                    },
                    config.acp.vera_comet_address.clone(),
                    signer_key_bytes,
                    &config.acp.vera_chain_id,
                    &tuning,
                    edge,
                )
                .map_err(|e| Error::InvalidConfig(format!("Vera provider: {}", e)))?,
            );

            let document_acp = vera::VeraDocumentACP::tuned(provider, &tuning, Some(edge));
            let document_acp = if config.acp.vera_events_ws.is_empty() {
                document_acp
            } else {
                document_acp
                    .with_cosmos_event_invalidation(config.acp.vera_events_ws.clone(), edge)
                    .map_err(|e| Error::InvalidConfig(format!("Vera event subscriber: {}", e)))?
            };
            let document_acp = Arc::new(document_acp);
            let dac_policies = db::DacPolicies::remote(document_acp.clone(), zanzibar_store);

            info!("Document ACP configured (Vera)");
            return Ok(DocumentAcpSetup {
                document_acp: gate_external_acp(document_acp, store, edge),
                dac_policies: Arc::new(dac_policies),
                #[cfg(feature = "embedded-vera")]
                embedded_vera: None,
            });
        }

        #[cfg(feature = "vera")]
        if config.acp.document_type == AcpDocumentType::VeraRs {
            if config.acp.vera_rs_address.is_empty() {
                return Err(Error::InvalidConfig(
                    "vera_rs_address required when document_type is verars".into(),
                ));
            }

            let signer_key_bytes = identity_key_bytes.ok_or_else(|| {
                Error::InvalidConfig(
                    "node identity required for vera.rs ACP (use --identity)".into(),
                )
            })?;
            let tuning = vera_rs_tuning(config);
            let deployment = config.acp.vera_deployment_id.ok_or_else(|| {
                Error::InvalidConfig(
                    "vera_deployment_id is required for native Vera submissions".into(),
                )
            })?;
            let worker = open_vera_worker(config, edge, deployment).await?;
            let provider = Arc::new(
                vera::VeraRsProvider::new(
                    config.acp.vera_rs_address.clone(),
                    &config.acp.vera_consensus_key,
                    signer_key_bytes,
                    worker,
                    &tuning,
                    Some(event_bus),
                    edge,
                )
                .await
                .map_err(|e| Error::InvalidConfig(format!("vera.rs provider: {}", e)))?,
            );

            info!("Document ACP configured (vera.rs)");
            return Ok(vera_rs_setup(
                provider,
                &tuning,
                store,
                zanzibar_store,
                edge,
            ));
        }

        #[cfg(feature = "embedded-vera")]
        if config.acp.document_type == AcpDocumentType::VeraRsEmbedded {
            return super::server_acp_embedded::setup(
                config,
                identity_key_bytes,
                store,
                zanzibar_store,
                event_bus,
                edge,
            )
            .await;
        }

        info!("Document ACP configured (local)");
        let document_acp = Arc::new(acp::ZanzibarDocumentACP::new(zanzibar_store.clone()));
        Ok(DocumentAcpSetup {
            document_acp,
            dac_policies: Arc::new(db::DacPolicies::local(zanzibar_store)),
            #[cfg(feature = "embedded-vera")]
            embedded_vera: None,
        })
    }

    pub(super) async fn setup_nac_manager<S: storage::corekv::Store + 'static>(
        config: &Config,
        user_did: Option<&Did>,
        store: Arc<S>,
        pool: Option<&workers::Pool>,
    ) -> Result<Option<Arc<defra_http::nac_adapter::NacAdapter>>> {
        if !config.acp.node_enable {
            return Ok(None);
        }

        let nac_config = db::NacConfig::new().with_enabled();
        let nac_manager: Arc<dyn db::NacManagerApi> = Arc::new(db::NacManager::new(
            Arc::new(acp::PersistentZanzibarStore::from_store(store)),
            nac_config,
        ));
        nac_manager
            .initialize(user_did)
            .await
            .map_err(|e| Error::InvalidConfig(format!("failed to initialize NAC: {}", e)))?;

        Ok(Some(Arc::new(
            defra_http::nac_adapter::NacAdapter::new(nac_manager).placed_on(pool),
        )))
    }
}

/// Gate a Vera provider with the registrations creates still owe it, and
/// settle in the background any a previous run left owed (a crash, or a
/// provider call that failed). Until settled, such a document is readable only
/// by its creator.
#[cfg(feature = "vera")]
fn gate_external_acp(
    provider: Arc<dyn acp::DocumentACP>,
    store: Arc<storage::DynStore>,
    edge: &edge::Edge,
) -> Arc<dyn acp::DocumentACP> {
    let gate = Arc::new(acp::ExternalDocumentAcp::new(provider, store));
    let redrive = Arc::clone(&gate);
    drop(edge.spawn(async move {
        match redrive.redrive_owed().await {
            Ok(report) if report.settled > 0 => info!(
                settled = report.settled,
                failed = report.failed,
                "Settled ACP registrations owed from a previous run"
            ),
            Ok(_) => {}
            Err(error) => tracing::warn!(
                %error,
                "Could not read owed ACP registrations; their documents stay readable only by their creators"
            ),
        }
    }));
    gate
}

/// The timeouts of a vera.rs provider, from the ACP configuration.
#[cfg(feature = "vera")]
pub(super) fn vera_rs_tuning(config: &Config) -> vera::AcpTuning {
    info!(
        request_timeout_s = config.acp.request_timeout,
        circuit_breaker_threshold = config.acp.circuit_breaker_threshold,
        circuit_breaker_reset_timeout_s = config.acp.circuit_breaker_reset_timeout,
        receipt_timeout_s = config.acp.receipt_timeout,
        "Resolved ACP tuning (vera.rs; access decision cache disabled)"
    );
    vera::AcpTuning {
        request_timeout: std::time::Duration::from_secs(config.acp.request_timeout),
        circuit_breaker_threshold: config.acp.circuit_breaker_threshold,
        circuit_breaker_reset_timeout: std::time::Duration::from_secs(
            config.acp.circuit_breaker_reset_timeout,
        ),
        cache_ttl: std::time::Duration::from_secs(config.acp.cache_ttl),
        receipt_timeout: std::time::Duration::from_secs(config.acp.receipt_timeout),
    }
}

/// The worker that journals this node's vera.rs submissions under
/// `<rootdir>/vera-worker`, with its key in the node's keyring.
#[cfg(feature = "vera")]
pub(super) async fn open_vera_worker(
    config: &Config,
    edge: &edge::Edge,
    deployment: u64,
) -> Result<vera::vera_rs::NativeWorker> {
    let worker_config = config.clone();
    edge.spawn_blocking(move || {
        let keyring = crate::commands::open_keyring(&worker_config)?;
        vera::vera_rs::NativeWorker::open(
            &worker_config.rootdir.join("vera-worker"),
            keyring.as_ref(),
            deployment,
        )
        .map_err(|e| Error::InvalidConfig(format!("Vera worker: {e}")))
    })
    .await
    .map_err(|e| Error::InvalidConfig(format!("Vera worker startup: {e}")))?
}

/// Document ACP over a vera.rs provider: verified decisions without an access
/// cache, remote policies mirrored locally, owed registrations gated.
#[cfg(feature = "vera")]
pub(super) fn vera_rs_setup(
    provider: Arc<vera::VeraRsProvider>,
    tuning: &vera::AcpTuning,
    store: Arc<storage::DynStore>,
    zanzibar_store: Arc<dyn acp::ZanzibarStore>,
    edge: &edge::Edge,
) -> DocumentAcpSetup {
    let document_acp = Arc::new(
        vera::VeraDocumentACP::without_access_cache(provider)
            .on_network(edge, tuning.request_timeout),
    );
    let dac_policies = db::DacPolicies::remote(document_acp.clone(), zanzibar_store);
    DocumentAcpSetup {
        document_acp: gate_external_acp(document_acp, store, edge),
        dac_policies: Arc::new(dac_policies),
        #[cfg(feature = "embedded-vera")]
        embedded_vera: None,
    }
}
