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

/// The node's NAC: its manager, which every check goes through, and whether
/// the NAC routes are served.
pub(super) struct NacSetup {
    pub(super) manager: Arc<dyn db::NacManagerApi>,
    pub(super) adapter: Arc<defra_http::nac_adapter::NacAdapter>,
    /// Enabled by the flag, or by a previous run on this store (disabled
    /// since included).
    pub(super) configured: bool,
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
                    vera_genesis(config)?,
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

    /// The node's NAC manager, always built and loaded from the store, as Go
    /// loads it whatever the flags (`db_nac.go`): a node a previous run
    /// enabled NAC on stays enforced when restarted without
    /// `--node-acp-enable`. The flag only enables a store not yet configured.
    pub(super) async fn setup_nac_manager<S: storage::corekv::Store + 'static>(
        config: &Config,
        user_did: Option<&Did>,
        store: Arc<S>,
        pool: Option<&workers::Pool>,
    ) -> Result<NacSetup> {
        let mut nac_config = db::NacConfig::new();
        if config.acp.node_enable {
            nac_config = nac_config.with_enabled();
        }
        let manager: Arc<dyn db::NacManagerApi> = Arc::new(db::NacManager::new(
            Arc::new(acp::PersistentZanzibarStore::from_store(store)),
            nac_config,
        ));
        manager
            .initialize(user_did)
            .await
            .map_err(|e| Error::InvalidConfig(format!("failed to initialize NAC: {}", e)))?;
        // The defra-node predicate: a store never configured gets no NAC
        // routes, so `/acp/node/enable` cannot hand an open node to its first
        // caller.
        let configured =
            config.acp.node_enable || manager.status().await != acp::nac::NacStatus::NotConfigured;

        Ok(NacSetup {
            adapter: Arc::new(
                defra_http::nac_adapter::NacAdapter::new(Arc::clone(&manager)).placed_on(pool),
            ),
            manager,
            configured,
        })
    }
}

/// Gate a Vera provider with the registrations creates still owe it, and
/// settle in the background any a previous run left owed (a crash, or a
/// provider call that failed). Until settled, such a document is readable only
/// by its creator. The relationship changes a previous run left owed are
/// submitted after them, once.
#[cfg(feature = "vera")]
fn gate_external_acp(
    provider: Arc<dyn acp::DocumentACP>,
    store: Arc<storage::DynStore>,
    edge: &edge::Edge,
) -> Arc<acp::ExternalDocumentAcp<storage::DynStore>> {
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
        match redrive.settle_owed_changes_once(acp::OwedScope::All).await {
            Ok(settlement) if settlement != acp::OwedSettlement::default() => info!(
                settled = settlement.settled,
                refused = settlement.refused,
            discarded = settlement.discarded,
            failed = settlement.failed,
                awaiting_authority = settlement.awaiting_authority,
                left = settlement.left,
                provider_unavailable = settlement.provider_unavailable,
                "Submitted ACP relationship changes owed from a previous run"
            ),
            Ok(_) => {}
            Err(error) => tracing::warn!(
                %error,
                "Could not read owed ACP relationship changes; they stay owed"
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

/// Where the vera.rs provider takes its deployment's genesis identifier from:
/// `vera_genesis_id`, hexadecimal, or nowhere. Without one the provider
/// refuses every term above 0, so after a switch of the deployment's active
/// device every verified read fails until it is configured.
#[cfg(feature = "vera")]
fn vera_genesis(config: &Config) -> Result<vera::vera_rs::GenesisSource> {
    let configured = config.acp.vera_genesis_id.trim();
    if configured.is_empty() {
        return Ok(vera::vera_rs::GenesisSource::Unnamed);
    }
    let bytes = hex::decode(configured.strip_prefix("0x").unwrap_or(configured))
        .map_err(|e| Error::InvalidConfig(format!("vera_genesis_id: {e}")))?;
    let id = <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        Error::InvalidConfig(format!("vera_genesis_id: {} bytes, not 32", bytes.len()))
    })?;
    Ok(vera::vera_rs::GenesisSource::Configured(id))
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
        vera::VeraDocumentACP::without_access_cache(provider.clone())
            .on_network(edge, tuning.request_timeout),
    );
    let dac_policies = db::DacPolicies::remote(document_acp.clone(), zanzibar_store);
    let gate = gate_external_acp(document_acp, store, edge);
    // What a switch of the deployment's active device voided is owed again
    // through the gate's outbox.
    let owed = Arc::downgrade(&gate);
    let owed: std::sync::Weak<dyn acp::OweAgain> = owed;
    vera::VeraProvider::owe_voided_to(provider.as_ref(), owed);
    DocumentAcpSetup {
        document_acp: gate,
        dac_policies: Arc::new(dac_policies),
        #[cfg(feature = "embedded-vera")]
        embedded_vera: None,
    }
}

#[cfg(all(test, feature = "vera"))]
mod tests {
    use super::*;

    fn genesis(configured: &str) -> Result<vera::vera_rs::GenesisSource> {
        let mut config = Config::default();
        config.acp.vera_genesis_id = configured.into();
        vera_genesis(&config)
    }

    #[test]
    fn a_remote_node_without_a_genesis_identifier_refuses_terms_above_zero() {
        assert_eq!(genesis("").unwrap(), vera::vera_rs::GenesisSource::Unnamed);
        assert_eq!(
            genesis(&format!("0x{}", "ab".repeat(32))).unwrap(),
            vera::vera_rs::GenesisSource::Configured([0xab; 32])
        );
        assert_eq!(
            genesis(&"01".repeat(32)).unwrap(),
            vera::vera_rs::GenesisSource::Configured([1; 32])
        );
        for invalid in ["not-hex", "abcd"] {
            assert!(matches!(
                genesis(invalid),
                Err(Error::InvalidConfig(message)) if message.contains("vera_genesis_id")
            ));
        }
    }
}

#[cfg(test)]
mod nac_tests {
    use super::*;
    use acp::nac::{NacStatus, NodePermission};
    use identity::Identity as _;

    const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

    struct Fixture {
        pool: workers::Pool,
        store: Arc<storage::RegolithStore>,
    }

    impl Fixture {
        fn new() -> Self {
            let pool = workers::Pool::start(storage::with_worker_io(workers::Config::unplaced(1)))
                .unwrap();
            let options = storage::RegolithStoreOptions::memory().placed_on(&pool);
            let store = Arc::new(
                storage::RegolithStore::open_with_options("regolith-memory", options).unwrap(),
            );
            Self { pool, store }
        }

        /// One `defradb start` on this store, with or without the flag.
        async fn start(&self, node_enable: bool, owner: Option<&Did>) -> NacSetup {
            let mut config = Config::default();
            config.acp.node_enable = node_enable;
            Node::setup_nac_manager(&config, owner, Arc::clone(&self.store), Some(&self.pool))
                .await
                .unwrap()
        }
    }

    fn stranger() -> Did {
        identity::RawIdentity::from_private_key(crypto::generate_ed25519().unwrap())
            .unwrap()
            .did()
            .unwrap()
    }

    #[test]
    fn stored_nac_is_enforced_after_a_restart_without_the_flag() {
        let fixture = Fixture::new();
        let owner = Did::new(OWNER).unwrap();
        workers::block_on(async {
            let first = fixture.start(true, Some(&owner)).await;
            assert_eq!(first.manager.status().await, NacStatus::Enabled);
            drop(first);

            let restarted = fixture.start(false, None).await;
            assert_eq!(restarted.manager.status().await, NacStatus::Enabled);
            assert!(restarted.configured, "the NAC routes went unserved");
            assert!(
                !restarted
                    .manager
                    .check_permission(&stranger(), NodePermission::CollectionGet)
                    .await
                    .unwrap(),
                "a stranger passed the stored NAC"
            );
        })
        .unwrap();
        fixture.pool.shutdown_blocking().unwrap();
    }

    #[test]
    fn a_store_never_configured_serves_no_nac_routes() {
        let fixture = Fixture::new();
        workers::block_on(async {
            let setup = fixture.start(false, None).await;
            assert_eq!(setup.manager.status().await, NacStatus::NotConfigured);
            assert!(!setup.configured);
            assert!(setup
                .manager
                .check_permission(&stranger(), NodePermission::CollectionGet)
                .await
                .unwrap());
        })
        .unwrap();
        fixture.pool.shutdown_blocking().unwrap();
    }

    #[test]
    fn a_disabled_nac_keeps_its_routes_after_a_restart_without_the_flag() {
        let fixture = Fixture::new();
        let owner = Did::new(OWNER).unwrap();
        workers::block_on(async {
            let first = fixture.start(true, Some(&owner)).await;
            first
                .manager
                .disable_as(&db::NacCaller {
                    identity: Some(owner.clone()),
                    node: None,
                })
                .await
                .unwrap();
            drop(first);

            let restarted = fixture.start(false, None).await;
            assert_eq!(
                restarted.manager.status().await,
                NacStatus::DisabledTemporarily
            );
            assert!(restarted.configured, "re-enable went unserved");
        })
        .unwrap();
        fixture.pool.shutdown_blocking().unwrap();
    }
}
