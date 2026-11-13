//! Document ACP over a Vera node this node runs in its own process.

use std::sync::Arc;

use crate::node_acp::{verified_acp_setup, DocumentAcpSetup};
use crate::{EmbeddedVeraConfig, Error, Result};

/// Starts the Vera node, then builds document ACP on its in-process
/// transport. The node is returned with the setup and stopped by
/// [`Node::shutdown`](crate::Node::shutdown); a setup that fails after the
/// node started drops it, which tells it to stop.
///
/// The provider trusts the consensus key of the deployment this process
/// created or reopened, and verifies every receipt, proof and header against
/// it as it does over the network. Once the node stops, by shutdown or on
/// its own, its transport refuses every call, so access decisions fail and
/// none is served from memory.
pub(crate) async fn start<S>(
    config: &EmbeddedVeraConfig,
    event_bus: Option<Arc<dyn events::Bus>>,
    edge: &edge::Edge,
    store: Arc<S>,
) -> Result<DocumentAcpSetup>
where
    S: storage::corekv::Store + 'static,
{
    let node = vera_embedded::HostedNode::start(
        vera_embedded::HostConfig {
            data_dir: config.data_dir.clone(),
            deployment: config.deployment.clone(),
            listen: config.listen,
            stop_deadline: config.stop_deadline,
        },
        edge,
    )
    .await?;
    let threads = node.threads();
    tracing::info!(
        chain_id = node.chain_id(),
        persistent_threads = threads.persistent(),
        max_threads = threads.max(),
        "embedded Vera node started"
    );
    let worker = {
        let (directory, keyring, deployment) = (
            config.worker_dir.clone(),
            Arc::clone(&config.keyring),
            node.chain_id(),
        );
        edge.spawn_blocking(move || {
            vera::vera_rs::NativeWorker::open(&directory, keyring.as_ref(), deployment)
        })
        .await
        .map_err(Error::EdgeRuntime)?
        .map_err(Error::VeraProvider)?
    };
    let transport = Arc::new(node.transport(config.tuning.request_timeout));
    let provider = Arc::new(
        vera::VeraRsProvider::with_transport(
            transport.clone(),
            transport,
            node.consensus_key(),
            &config.actor_key,
            worker,
            &config.tuning,
            event_bus,
            edge,
        )
        .await
        .map_err(Error::VeraProvider)?,
    );
    let mut setup = verified_acp_setup(provider, edge, config.tuning.request_timeout, store);
    setup.embedded_vera = Some(node);
    Ok(setup)
}
