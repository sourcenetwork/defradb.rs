//! Background event loop owning all iroh state.
//!
//! `IrohEndpoint` runs as a spawned tokio task and processes:
//! - Incoming QUIC connections
//! - The gossip driver, which rides those connections
//! - Commands from the `IrohTransport` facade

use rapidhash::fast::RandomState;
use rapidhash::{HashMapExt, RapidHashMap};
use std::future::Future;
use std::sync::Arc;

use iroh::Endpoint;
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use n0_future::task::JoinHandle;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

use defra_core::thread_bounds::MaybeSend;

use crate::bitswap::ReplicatorRegistry;
use crate::message::PushLogReply;
use crate::tracked_task::TrackedAbort;
use crate::transport::{PeerAddr, PeerId, TransportEvent};

use super::command::IrohCommand;
use super::endpoint_commands::{handle_command, GossipDialer};
use super::endpoint_config::{
    allowlist_state_from_config, apply_bind_config, apply_discovery_config, apply_transport_config,
    relay_mode_from_config, IrohEndpointConfig, PeerAdmission,
};
use super::endpoint_rpc::{new_connection_cache, ConnectionCache};
use super::endpoint_streams::handle_incoming;
use super::fetch_stats::FetchStats;
use super::gossip::GossipHandle;
use super::inbound_budget::InboundBudget;
use super::peer_map::{parse_endpoint_id, SharedPeerMap};
use super::protocols;
use super::task_registry::TaskRegistry;

/// Events each dispatch class may queue before its senders wait.
const EVENT_LANE_CAPACITY: usize = 64;

const MAX_COMMAND_BATCH: usize = 16;

#[cfg(test)]
#[path = "../../tests/iroh/task_shutdown.rs"]
mod task_shutdown_tests;

/// Shared handles to the endpoint's long-lived state, cloneable into spawned
/// tasks (dials, incoming connections).
#[derive(Clone)]
pub(super) struct EndpointResources {
    pub(super) endpoint: Endpoint,
    pub(super) gossip: GossipHandle,
    pub(super) peer_map: Arc<SharedPeerMap>,
    pub(super) connection_cache: ConnectionCache,
    pub(super) spawned_tasks: SpawnedTasks,
    pub(super) node_identity: Option<Arc<identity::RawIdentity>>,
    pub(super) admission: Arc<PeerAdmission>,
    pub(super) inbound_budget: Arc<InboundBudget>,
    pub(super) fetch_stats: Arc<FetchStats>,
}

pub(super) type RawTopics = Arc<HopscotchMap<String, (), RandomState>>;

/// Active block sync task.
pub(super) struct ActiveSync {
    pub(super) abort_handle: TrackedAbort,
}

pub(super) type SpawnedTasks = Arc<TaskRegistry>;
/// A reply sender parked in a queue so the responder can take ownership of it.
pub(super) type PushLogReplySlot = Arc<SegQueue<oneshot::Sender<PushLogReply>>>;
pub(super) type PendingPushLogReplies = Arc<HopscotchMap<String, PushLogReplySlot, RandomState>>;

pub(super) fn spawn_task(
    spawned_tasks: &SpawnedTasks,
    future: impl Future<Output = ()> + MaybeSend + 'static,
) -> Option<TrackedAbort> {
    spawned_tasks.spawn(future)
}

async fn shutdown_tracked_tasks(spawned_tasks: SpawnedTasks) {
    let tasks = spawned_tasks.close();
    let task_count = tasks.len();
    let drain = async move {
        for task in tasks {
            let _ = task.await;
        }
    };
    if n0_future::time::timeout(std::time::Duration::from_secs(5), drain)
        .await
        .is_err()
    {
        warn!(
            task_count,
            "Timed out draining Iroh tasks after cancellation"
        );
    }
}

/// Spawn the iroh endpoint background task.
///
/// Returns the command sender, event receiver, and background task handle.
pub async fn spawn_endpoint(
    config: IrohEndpointConfig,
) -> crate::error::Result<(
    mpsc::Sender<IrohCommand>,
    crate::sync::DispatchReceiver<TransportEvent<iroh::endpoint::SendStream>>,
    Arc<ReplicatorRegistry>,
    JoinHandle<()>,
)> {
    // Every Defra protocol, gossip included, is multiplexed over one ALPN. The
    // version before gossip moved onto it is still served, without gossip.
    let alpns: Vec<Vec<u8>> = vec![
        protocols::ALPN_MUX.to_vec(),
        protocols::ALPN_MUX_V1.to_vec(),
    ];

    let relay_mode = relay_mode_from_config(&config.relay_mode)?;
    let admission = Arc::new(PeerAdmission::new(allowlist_state_from_config(
        &config.allowlist,
    )?));
    let node_identity = config.node_identity.clone();
    let relay_urls: Vec<String> = relay_mode
        .relay_map()
        .urls::<Vec<_>>()
        .iter()
        .map(|u| u.to_string())
        .collect();
    tracing::info!(?relay_urls, "iroh endpoint relay configuration");
    let mut builder = Endpoint::builder(iroh::endpoint::presets::Minimal)
        .relay_mode(relay_mode)
        .secret_key(config.secret_key.clone())
        .alpns(alpns);
    builder = apply_transport_config(builder, config.max_concurrent_multipath_paths)?;
    builder = apply_discovery_config(builder, &config.discovery)?;
    builder = apply_bind_config(builder, config.bind_addr, config.bind_port)?;

    let endpoint = builder.bind().await.map_err(|e| {
        crate::error::Error::Transport(format!("failed to bind iroh endpoint: {}", e))
    })?;

    let (command_tx, command_rx) = mpsc::channel::<IrohCommand>(256);
    let (event_tx, event_rx) = crate::sync::dispatch_queue::<
        TransportEvent<iroh::endpoint::SendStream>,
    >(EVENT_LANE_CAPACITY);
    let replicators = Arc::new(ReplicatorRegistry::new());

    let task = n0_future::task::spawn(run_event_loop(
        endpoint,
        node_identity,
        admission,
        command_rx,
        event_tx,
        replicators.clone(),
    ));

    Ok((command_tx, event_rx, replicators, task))
}

#[cfg(test)]
#[path = "../../tests/unit/iroh_endpoint_lifecycle.rs"]
mod lifecycle_tests;

/// Main event loop processing incoming connections, gossip, and commands.
#[allow(clippy::too_many_arguments)]
async fn run_event_loop(
    endpoint: Endpoint,
    node_identity: Option<Arc<identity::RawIdentity>>,
    admission: Arc<PeerAdmission>,
    mut command_rx: mpsc::Receiver<IrohCommand>,
    event_tx: crate::sync::DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
    replicators: Arc<ReplicatorRegistry>,
) {
    let shutdown_started = web_time::Instant::now();
    let peer_map = Arc::new(SharedPeerMap::new());
    let pending_pushlog_replies: PendingPushLogReplies =
        Arc::new(HopscotchMap::with_hasher(RandomState::default()));
    let gossip = GossipHandle::new();
    let connection_cache = new_connection_cache(gossip.clone());
    let raw_topics: RawTopics = Arc::new(HopscotchMap::with_hasher(RandomState::default()));
    let mut active_syncs: RapidHashMap<u64, ActiveSync> = RapidHashMap::new();
    let spawned_tasks: SpawnedTasks = Arc::new(TaskRegistry::default());
    let mut next_query_id: u64 = 1;

    let resources = EndpointResources {
        endpoint: endpoint.clone(),
        gossip: gossip.clone(),
        peer_map: Arc::clone(&peer_map),
        connection_cache: Arc::clone(&connection_cache),
        spawned_tasks: Arc::clone(&spawned_tasks),
        node_identity,
        admission,
        inbound_budget: Arc::default(),
        fetch_stats: Arc::default(),
    };
    gossip.start(
        endpoint.id(),
        GossipDialer::new(&resources, &pending_pushlog_replies, &event_tx),
        event_tx.clone(),
        Arc::clone(&raw_topics),
        Arc::clone(&spawned_tasks),
    );

    // Emit Listening event with our endpoint address
    let addr_str = format!("iroh://{}", endpoint.id());
    if event_tx
        .send(TransportEvent::Listening(PeerAddr::new(addr_str)))
        .await
        .is_err()
    {
        warn!("Event channel closed, cannot emit Listening event");
    }

    'endpoint: loop {
        if command_rx.is_closed() {
            break;
        }
        tokio::select! {
            cmd = command_rx.recv() => {
                let Some(cmd) = cmd else { break };
                let mut pending_cmd = Some(cmd);
                let mut processed = 0;
                while let Some(cmd) = pending_cmd.take() {
                    // Closing a channel does not discard its buffered commands.
                    if command_rx.is_closed() {
                        break 'endpoint;
                    }
                    let should_shutdown = handle_command(
                        cmd,
                        &resources,
                        &pending_pushlog_replies,
                        &raw_topics,
                        &replicators,
                        &mut active_syncs,
                        &mut next_query_id,
                        &event_tx,
                    ).await;
                    if should_shutdown {
                        break 'endpoint;
                    }

                    processed += 1;
                    if processed >= MAX_COMMAND_BATCH {
                        break;
                    }
                    pending_cmd = command_rx.try_recv().ok();
                }
            }
            incoming = endpoint.accept() => {
                match incoming {
                    Some(incoming) => {
                        let resources = resources.clone();
                        let pending_pushlog_replies = Arc::clone(&pending_pushlog_replies);
                        let event_tx = event_tx.clone();
                        let _ = spawn_task(&spawned_tasks, async move {
                            handle_incoming(
                                incoming,
                                &resources,
                                &pending_pushlog_replies,
                                &event_tx,
                            )
                            .await;
                        });
                    }
                    None => break,
                }
            }
            else => break,
        }
    }

    // Reject queued and new commands before waiting for network teardown.
    drop(command_rx);

    gossip.close();

    let syncs_started = web_time::Instant::now();
    for (_, sync) in active_syncs.drain() {
        sync.abort_handle.abort();
    }
    warn!(
        elapsed_ms = syncs_started.elapsed().as_millis(),
        "Iroh endpoint shutdown: active syncs aborted"
    );

    let tracked_started = web_time::Instant::now();
    shutdown_tracked_tasks(spawned_tasks).await;
    debug!(
        elapsed_ms = tracked_started.elapsed().as_millis(),
        "Iroh endpoint shutdown: task drain finished"
    );

    let close_started = web_time::Instant::now();
    endpoint.close().await;
    warn!(
        close_elapsed_ms = close_started.elapsed().as_millis(),
        total_elapsed_ms = shutdown_started.elapsed().as_millis(),
        "Iroh endpoint shut down"
    );
}

/// Look up the cached direct socket address for a peer from the peer map.
pub(super) fn peer_direct_addr(
    peer_map: &SharedPeerMap,
    peer_id: &PeerId,
) -> Option<std::net::SocketAddr> {
    let id = parse_endpoint_id(peer_id).ok()?;
    peer_map.remote_addr(&id)
}
