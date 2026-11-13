//! Document ACP over a Vera node the `defradb` process runs itself.

use std::sync::Arc;
use std::time::Duration;

use tracing::info;

use super::server_acp::{gate_external_acp, DocumentAcpSetup};
use crate::config::Config;
use crate::error::{Error, Result};

/// How long the start waits for the Vera node, and shutdown for its stop.
const START_DEADLINE: Duration = Duration::from_secs(60);
const STOP_DEADLINE: Duration = Duration::from_secs(30);

/// Starts a single-validator Vera node under `<rootdir>/vera` and builds
/// document ACP on its in-process transport. The node identity is the actor
/// that owns this node's policies and, unless operators are configured, the
/// deployment's only operator. Every read is verified against the consensus
/// key of the deployment this process created or reopened.
pub(super) async fn setup(
    config: &Config,
    identity_key_bytes: Option<&[u8]>,
    store: Arc<storage::DynStore>,
    zanzibar_store: Arc<dyn acp::ZanzibarStore>,
    event_bus: Arc<dyn events::Bus>,
    edge: &edge::Edge,
) -> Result<DocumentAcpSetup> {
    let invalid = |what: &str, error: &dyn std::fmt::Display| {
        Error::InvalidConfig(format!("embedded Vera {what}: {error}"))
    };
    let actor_key = identity_key_bytes.ok_or_else(|| {
        Error::InvalidConfig("node identity required for embedded Vera ACP (use --identity)".into())
    })?;
    let chain_id = config.acp.vera_deployment_id.ok_or_else(|| {
        Error::InvalidConfig("vera_deployment_id is required for an embedded Vera node".into())
    })?;
    let listen = config
        .acp
        .vera_embedded_listen
        .parse()
        .map_err(|error| invalid("listen address", &error))?;
    let mut operator_keys = config.acp.vera_embedded_operators.clone();
    if operator_keys.is_empty() {
        let identity = vera::vera_rs::Secp256k1Signer::from_slice(actor_key)
            .map_err(|error| invalid("operator key", &error))?;
        operator_keys.push(identity.operator_key());
    }
    // vera.rs takes the keys in ascending order.
    operator_keys.sort();
    let tuning = vera::AcpTuning {
        request_timeout: Duration::from_secs(config.acp.request_timeout),
        circuit_breaker_threshold: config.acp.circuit_breaker_threshold,
        circuit_breaker_reset_timeout: Duration::from_secs(
            config.acp.circuit_breaker_reset_timeout,
        ),
        cache_ttl: Duration::from_secs(config.acp.cache_ttl),
        receipt_timeout: Duration::from_secs(config.acp.receipt_timeout),
    };

    let node = vera_embedded::HostedNode::start(
        vera_embedded::HostConfig {
            data_dir: config.rootdir.join("vera"),
            deployment: vera_embedded::Deployment {
                chain_id,
                threshold: config.acp.vera_embedded_threshold.unwrap_or(1),
                operator_keys,
                created: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|error| invalid("genesis time", &error))?
                    .as_secs(),
            },
            listen,
            start_deadline: START_DEADLINE,
            stop_deadline: STOP_DEADLINE,
        },
        edge,
    )
    .await
    .map_err(|error| invalid("node", &error))?;
    let threads = node.threads();
    info!(
        chain_id,
        persistent_threads = threads.persistent(),
        max_threads = threads.max(),
        "Embedded Vera node started"
    );

    let worker_config = config.clone();
    let worker = edge
        .spawn_blocking(move || {
            let keyring = crate::commands::open_keyring(&worker_config)?;
            vera::vera_rs::NativeWorker::open(
                &worker_config.rootdir.join("vera-worker"),
                keyring.as_ref(),
                chain_id,
            )
            .map_err(|e| Error::InvalidConfig(format!("Vera worker: {e}")))
        })
        .await
        .map_err(|e| Error::InvalidConfig(format!("Vera worker startup: {e}")))??;
    let transport = Arc::new(node.transport(tuning.request_timeout));
    let provider = Arc::new(
        vera::VeraRsProvider::with_transport(
            transport.clone(),
            transport,
            node.consensus_key(),
            actor_key,
            worker,
            &tuning,
            Some(event_bus),
            edge,
        )
        .await
        .map_err(|error| invalid("provider", &error))?,
    );
    let document_acp = Arc::new(
        vera::VeraDocumentACP::without_access_cache(provider)
            .on_network(edge, tuning.request_timeout),
    );
    let dac_policies = db::DacPolicies::remote(document_acp.clone(), zanzibar_store);

    info!("Document ACP configured (embedded vera.rs)");
    Ok(DocumentAcpSetup {
        document_acp: gate_external_acp(document_acp, store, edge),
        dac_policies: Arc::new(dac_policies),
        embedded_vera: Some(node),
    })
}
