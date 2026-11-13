//! Document ACP over a Vera node the `defradb` process runs itself.

use std::sync::Arc;
use std::time::Duration;

use tracing::info;

use super::server_acp::{open_vera_worker, vera_rs_setup, vera_rs_tuning, DocumentAcpSetup};
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
    let tuning = vera_rs_tuning(config);

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

    let worker = open_vera_worker(config, edge, chain_id).await?;
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
    info!("Document ACP configured (embedded vera.rs)");
    let mut setup = vera_rs_setup(provider, &tuning, store, zanzibar_store, edge);
    setup.embedded_vera = Some(node);
    Ok(setup)
}
