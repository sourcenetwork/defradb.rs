use crate::Error;
use crate::Persistence;
use crate::Result;
use std::sync::Arc;

pub(crate) struct DocumentAcpSetup {
    pub document_acp: Arc<dyn acp::DocumentACP>,
    pub local_zanzibar_store: Option<Arc<dyn acp::ZanzibarStore>>,
    /// The node's DAC policies, installed on the database.
    pub dac_policies: Arc<db::DacPolicies>,
    #[cfg(feature = "vera")]
    pub vera_acp: Option<Arc<vera::VeraDocumentACP>>,
    /// The gate used by the node’s settlement and cleanup tasks.
    #[cfg(feature = "vera")]
    pub redrive: Option<Arc<acp::ExternalDocumentAcp<storage::RegolithStore>>>,
    /// The Vera node started for document ACP; the node stops it on shutdown.
    #[cfg(feature = "embedded-vera")]
    pub embedded_vera: Option<vera_embedded::HostedNode>,
}

pub(crate) async fn create_document_acp(
    store: Arc<storage::RegolithStore>,
    _persistence: Persistence,
    config: &crate::DocumentAcpConfig,
    #[cfg_attr(not(feature = "vera"), allow(unused_variables))] network: Option<&edge::Edge>,
    #[cfg_attr(not(feature = "vera"), allow(unused_variables))] event_bus: Option<
        Arc<dyn events::Bus>,
    >,
) -> Result<DocumentAcpSetup> {
    match config {
        #[cfg(feature = "vera")]
        crate::DocumentAcpConfig::Vera(config) => {
            let edge = remote_edge(network)?;
            let worker = {
                let (directory, keyring, deployment) = (
                    config.worker_dir.clone(),
                    Arc::clone(&config.keyring),
                    config.deployment_id,
                );
                edge.spawn_blocking(move || {
                    vera::vera_rs::NativeWorker::open(&directory, keyring.as_ref(), deployment)
                })
                .await
                .map_err(Error::EdgeRuntime)?
                .map_err(Error::VeraProvider)?
            };
            let provider = match &config.actor {
                crate::VeraActor::Secp256k1Key(key) => {
                    vera::VeraRsProvider::new(
                        config.rpc_address.clone(),
                        &config.consensus_key,
                        genesis_source(config.genesis_id),
                        key,
                        worker,
                        &config.tuning,
                        event_bus,
                        &edge,
                    )
                    .await
                }
                crate::VeraActor::Signer(signer) => {
                    vera::VeraRsProvider::new_with_signer(
                        config.rpc_address.clone(),
                        &config.consensus_key,
                        genesis_source(config.genesis_id),
                        Arc::clone(signer),
                        worker,
                        &config.tuning,
                        event_bus,
                        &edge,
                    )
                    .await
                }
            };
            let provider = Arc::new(provider.map_err(Error::VeraProvider)?);
            Ok(verified_acp_setup(
                provider,
                &edge,
                config.tuning.request_timeout,
                store,
            ))
        }
        #[cfg(feature = "vera")]
        crate::DocumentAcpConfig::Provider(host) => Ok(verified_acp_setup(
            Arc::clone(&host.provider),
            &remote_edge(network)?,
            host.request_timeout,
            store,
        )),
        #[cfg(feature = "embedded-vera")]
        crate::DocumentAcpConfig::EmbeddedVera(config) => {
            let edge = remote_edge(network)?;
            crate::embedded_vera::start(config, event_bus, &edge, store).await
        }
        crate::DocumentAcpConfig::Local => {
            // Memory and disk profiles both share the engine with document rows.
            let zanzibar_store = Arc::new(acp::PersistentZanzibarStore::from_store(store));
            let document_acp = Arc::new(acp::ZanzibarDocumentACP::new(zanzibar_store.clone()));
            Ok(local_document_acp_setup(document_acp, zanzibar_store))
        }
    }
}

/// Where a remote Vera node's provider takes its deployment's genesis
/// identifier from: the configuration, or nowhere (every term above 0 is
/// then refused).
#[cfg(feature = "vera")]
fn genesis_source(configured: Option<[u8; 32]>) -> vera::vera_rs::GenesisSource {
    configured.map_or(
        vera::vera_rs::GenesisSource::Unnamed,
        vera::vera_rs::GenesisSource::Configured,
    )
}

/// The edge the Vera provider's connections run on: the node's, or for a
/// node that runs no other network work, the process's on-demand one.
#[cfg(feature = "vera")]
fn remote_edge(network: Option<&edge::Edge>) -> Result<edge::Edge> {
    match network {
        Some(edge) => Ok(edge.clone()),
        None => edge::on_demand().cloned().map_err(Error::EdgeRuntime),
    }
}

/// Document ACP over a provider whose reads are verified: access decisions
/// run on `edge` and are not cached, the provider's policies are mirrored into
/// `store`, and owed registrations are gated and redriven.
#[cfg(feature = "vera")]
pub(crate) fn verified_acp_setup(
    provider: Arc<dyn vera::VeraProvider>,
    edge: &edge::Edge,
    request_timeout: std::time::Duration,
    store: Arc<storage::RegolithStore>,
) -> DocumentAcpSetup {
    let vera_acp = Arc::new(
        vera::VeraDocumentACP::new(Arc::clone(&provider)).on_network(edge, request_timeout),
    );
    let dac_policies = vera_policies(&vera_acp, &store);
    let gate = Arc::new(acp::ExternalDocumentAcp::new(vera_acp.clone(), store));
    // What a switch of the deployment's active device voided is owed again
    // through the gate's outbox.
    let owed = Arc::downgrade(&gate);
    let owed: std::sync::Weak<dyn acp::OweAgain> = owed;
    provider.owe_voided_to(owed);
    let recovery = Arc::downgrade(&gate);
    let recovery: std::sync::Weak<dyn acp::RecoverySource> = recovery;
    provider.set_recovery_source(recovery);
    DocumentAcpSetup {
        document_acp: gate.clone(),
        local_zanzibar_store: None,
        dac_policies,
        vera_acp: Some(vera_acp),
        redrive: Some(gate),
        #[cfg(feature = "embedded-vera")]
        embedded_vera: None,
    }
}

/// The policies Vera owns, each one the node uses copied into `store`.
#[cfg(feature = "vera")]
fn vera_policies<S>(vera: &Arc<vera::VeraDocumentACP>, store: &Arc<S>) -> Arc<db::DacPolicies>
where
    S: storage::corekv::Store + 'static,
{
    let mirror = Arc::new(acp::PersistentZanzibarStore::from_store(Arc::clone(store)));
    Arc::new(db::DacPolicies::remote(vera.clone(), mirror))
}

/// The node's NAC manager. With `owner`, NAC is enabled at startup and owned
/// by it; without, NAC stays off until enabled at runtime. A status change
/// that loses its commit re-runs up to `max_txn_retries` times, as the
/// database's transactions do.
pub(crate) async fn create_nac_manager<S>(
    store: Arc<S>,
    owner: Option<&identity::Did>,
    max_txn_retries: u32,
) -> Result<Arc<dyn db::NacManagerApi>>
where
    S: storage::corekv::Store + 'static,
{
    let nac_store = Arc::new(acp::PersistentZanzibarStore::from_store(store));
    let mut nac_config = db::NacConfig::new()
        .with_dev_mode()
        .with_max_txn_retries(max_txn_retries);
    if owner.is_some() {
        nac_config = nac_config.with_enabled();
    }
    let manager = Arc::new(db::NacManager::new_bound(nac_store, nac_config));
    manager.initialize(owner).await.map_err(Error::NacInit)?;
    Ok(manager)
}

fn local_document_acp_setup(
    document_acp: Arc<dyn acp::DocumentACP>,
    zanzibar_store: Arc<dyn acp::ZanzibarStore>,
) -> DocumentAcpSetup {
    DocumentAcpSetup {
        document_acp,
        dac_policies: Arc::new(db::DacPolicies::local(zanzibar_store.clone())),
        local_zanzibar_store: Some(zanzibar_store),
        #[cfg(feature = "vera")]
        vera_acp: None,
        #[cfg(feature = "vera")]
        redrive: None,
        #[cfg(feature = "embedded-vera")]
        embedded_vera: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{create_document_acp, Persistence};
    use crate::DocumentAcpConfig;
    use acp::{DocumentPermission, Identity, StorePolicyOptions};
    use identity::Did;
    use std::sync::Arc;

    fn did(value: &str) -> Did {
        Did::new(value).unwrap()
    }

    #[cfg(feature = "vera")]
    #[test]
    fn vera_document_acp_rejects_an_invalid_trusted_key_before_connecting() {
        edge::testing::edge()
            .block_on(async {
                let directory = tempfile::tempdir().unwrap();
                let keyring: Arc<dyn keyring::Keyring> = Arc::new(
                    keyring::FileKeyring::open(directory.path().join("keys"), b"test-password")
                        .unwrap(),
                );
                let worker_dir = directory.path().join("worker");
                let config = DocumentAcpConfig::Vera(crate::VeraConfig {
                    // Nothing listens here; a connection attempt would fail differently.
                    rpc_address: "http://127.0.0.1:1".into(),
                    consensus_key: "not-a-key".into(),
                    genesis_id: None,
                    deployment_id: 9001,
                    actor: crate::VeraActor::Secp256k1Key(vec![7; 32]),
                    worker_dir: worker_dir.clone(),
                    keyring,
                    tuning: vera::AcpTuning::default(),
                });
                let store = Arc::new(storage::RegolithStore::in_memory().unwrap());
                let result = create_document_acp(
                    store,
                    Persistence::Memory,
                    &config,
                    Some(&edge::testing::edge()),
                    None,
                )
                .await;
                assert!(matches!(
                    result.err(),
                    Some(crate::Error::VeraProvider(vera::ProviderError::Config(message)))
                        if message.contains("invalid trusted consensus key")
                ));
                assert!(worker_dir.exists(), "the worker journal is opened first");
            })
            .unwrap()
    }

    #[test]
    fn local_document_acp_enforces_stored_custom_policy() {
        workers::block_on(async {
            let store = Arc::new(storage::RegolithStore::in_memory().unwrap());
            let acp_setup = create_document_acp(
                store,
                Persistence::Memory,
                &DocumentAcpConfig::Local,
                None,
                None,
            )
            .await
            .unwrap();

            #[cfg(feature = "vera")]
            assert!(acp_setup.vera_acp.is_none());
            let document_acp = acp_setup.document_acp;
            let local_store = acp_setup
                .local_zanzibar_store
                .expect("local ACP should expose a Zanzibar store");

            let parsed = acp::policy_yaml::parse_policy_yaml(
                r#"
name: Viewer Policy
resources:
  - name: users
    relations:
      - name: viewer
      - name: editor
      - name: remover
    permissions:
      - name: read
        expr: viewer
      - name: update
        expr: editor
      - name: delete
        expr: remover
"#,
            )
            .unwrap();
            let policy = acp::policy_yaml::build_policy(&parsed, 1).unwrap();
            let options = StorePolicyOptions::new()
                .with_validation()
                .with_dpi_enforcement();
            local_store
                .store_policy_with_options(&policy, &options)
                .await
                .unwrap();

            let owner = did("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK");
            let viewer = did("did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR");

            document_acp
                .register_doc_object(&owner, &policy.id, "users", "doc1", &[])
                .await
                .unwrap();
            document_acp
                .add_actor_relationship(&owner, &viewer, &policy.id, "users", "doc1", "viewer", &[])
                .await
                .unwrap();

            assert!(document_acp
                .check_doc_access(
                    &Identity::Authenticated(viewer.clone()),
                    DocumentPermission::Read,
                    &policy.id,
                    "users",
                    "doc1",
                )
                .await
                .unwrap());
            assert!(!document_acp
                .check_doc_access(
                    &Identity::Authenticated(viewer),
                    DocumentPermission::Update,
                    &policy.id,
                    "users",
                    "doc1",
                )
                .await
                .unwrap());
        })
        .unwrap()
    }
}
