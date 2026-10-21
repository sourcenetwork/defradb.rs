#[cfg(feature = "libp2p")]
use std::sync::Arc;

use kovan_queue::seg_queue::SegQueue;
#[cfg(feature = "libp2p")]
use p2p::sync::{ReplicationConfig, ReplicationLoop};

#[cfg(feature = "libp2p")]
use crate::node::EmbeddedMergeHandler;

pub struct BackgroundTasks {
    tasks: SegQueue<workers::Background>,
}

impl BackgroundTasks {
    pub(crate) fn new(tasks: impl IntoIterator<Item = workers::Background>) -> Self {
        let queue = SegQueue::new();
        for task in tasks {
            queue.push(task);
        }
        Self { tasks: queue }
    }

    /// Stop and await all node-owned background tasks.
    ///
    /// Awaiting the stop is important for persistent stores: a stopped task
    /// holds the final database handle until its worker next polls it, which
    /// otherwise leaves the on-disk database lock held after close.
    pub async fn shutdown(&self) {
        while let Some(task) = self.tasks.pop() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for BackgroundTasks {
    fn drop(&mut self) {
        while let Some(task) = self.tasks.pop() {
            task.abort();
        }
    }
}

#[cfg(feature = "libp2p")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_libp2p_event_handler<B, S>(
    events: tokio::sync::mpsc::Receiver<p2p::HostEvent>,
    coordinator: Arc<p2p::sync::Libp2pSyncCoordinator<B>>,
    store: Arc<S>,
    retry_peers: storage::stores::Peerstore<S>,
    event_bus: Arc<dyn events::Bus>,
    handle: p2p::P2PHostHandle,
    se_correlator: p2p::SeQueryCorrelator,
    manage_hooks: defra_p2p_adapter::manage::hooks::ManageHooksCell,
) -> tokio::task::JoinHandle<()>
where
    B: blockstore::Blockstore + 'static,
    S: storage::corekv::Store + 'static,
{
    tokio::spawn(async move {
        let handler_coordinator = coordinator.clone();
        coordinator.run_event_dispatcher(events, move |event, admission| {
            let coordinator = handler_coordinator.clone();
            let store = store.clone();
            let retry_peers = retry_peers.clone();
            let event_bus = event_bus.clone();
            let handle = handle.clone();
            let se_correlator = se_correlator.clone();
            let manage_hooks = manage_hooks.clone();
            async move {
            match &event {
                p2p::HostEvent::PeerConnected(peer_id) => {
                    defra_p2p_adapter::activate_retry_peer(
                        &retry_peers,
                        &p2p::transport::PeerId::from(*peer_id),
                    )
                    .await;
                    event_bus.publish(events::Message::peer_connection(
                        events::PeerConnectionData {
                            peer_id: peer_id.to_string(),
                            connected: true,
                        },
                    ));
                }
                p2p::HostEvent::PeerDisconnected(peer_id) => {
                    event_bus.publish(events::Message::peer_connection(
                        events::PeerConnectionData {
                            peer_id: peer_id.to_string(),
                            connected: false,
                        },
                    ));
                }
                p2p::HostEvent::PeerSubscribed { peer_id, topic } => {
                    event_bus.publish(events::Message::topic_peer_event(
                        events::TopicPeerEventData {
                            peer_id: peer_id.to_string(),
                            topic: topic.clone(),
                            event_type: "JOINED".to_string(),
                        },
                    ));
                }
                p2p::HostEvent::PeerUnsubscribed { peer_id, topic } => {
                    event_bus.publish(events::Message::topic_peer_event(
                        events::TopicPeerEventData {
                            peer_id: peer_id.to_string(),
                            topic: topic.clone(),
                            event_type: "LEFT".to_string(),
                        },
                    ));
                }
                _ => {}
            }

            let transport_event = p2p::convert_host_event(event);
            if admission == p2p::sync::DispatchAdmission::Saturated {
                if let Err(error) = coordinator
                    .handle_transport_event_with_admission(transport_event, admission)
                    .await
                {
                    tracing::debug!(%error, "rejected saturated embedded P2P request");
                }
                return;
            }
            let transport_event = match transport_event {
                p2p::TransportEvent::SEArtifactsReceived { peer_id, data } => {
                    if let Ok(pid) = peer_id.as_str().parse::<libp2p::PeerId>() {
                        // Stores artifacts AND sends the signed ack Go's push waits for.
                        let doc_ids = db::merge::se::serve::handle_artifacts_push(
                            store.as_ref(),
                            &handle,
                            pid,
                            &data,
                        )
                        .await;
                        for doc_id in doc_ids {
                            event_bus.publish(events::Message::se_artifact_received(
                                events::SEArtifactReceivedData { doc_id },
                            ));
                        }
                    } else {
                        handle_se_artifacts_received(
                            store.clone(),
                            event_bus.clone(),
                            peer_id.to_string(),
                            data,
                        )
                        .await;
                    }
                    return;
                }
                p2p::TransportEvent::SEQueryRequest {
 peer_id,
 request,
 token,
 } => {
                    let transport = p2p::Libp2pTransport::new(handle.clone());
                    db::merge::se::serve::handle_query_request(
                        store.as_ref(),
                        &transport,
                        peer_id,
                        request, token,
                    )
                    .await;
                    return;
                }
                p2p::TransportEvent::SEQueryReply { reply, .. } => {
                    se_correlator.deliver(reply);
                    return;
                }
                p2p::TransportEvent::ManageRequest {
 peer_id,
 request,
 token,
 } => {
                    if let Some(hooks) = manage_hooks.get() {
                        let transport = p2p::Libp2pTransport::new(handle.clone());
                        defra_p2p_adapter::manage::serve::serve_manage_request(
                            hooks, &transport, &peer_id, request, token,
                        )
                        .await;
                    } else {
                        tracing::debug!(%peer_id, "manage request before hooks ready; dropping");
                    }
                    return;
                }
                p2p::TransportEvent::ManageQueryRequest {
 peer_id,
 request,
 token,
 } => {
                    if let Some(hooks) = manage_hooks.get() {
                        let transport = p2p::Libp2pTransport::new(handle.clone());
                        defra_p2p_adapter::manage::serve::serve_manage_query_request(
                            hooks, &transport, &peer_id, request, token,
                        )
                        .await;
                    } else {
                        tracing::debug!(%peer_id, "manage query request before hooks ready; dropping");
                    }
                    return;
                }
                p2p::TransportEvent::ManageReply { reply, .. } => {
                    if let Some(hooks) = manage_hooks.get() {
                        hooks.correlator.deliver(reply);
                    }
                    return;
                }
                p2p::TransportEvent::ManageQueryReply { reply, .. } => {
                    if let Some(hooks) = manage_hooks.get() {
                        hooks.query_correlator.deliver(reply);
                    }
                    return;
                }
                other => other,
            };
            if let Err(error) = coordinator
                .handle_transport_event_with_admission(transport_event, admission)
                .await
            {
                tracing::error!(error = %error, "error handling libp2p event");
            }
            }
        })
        .await;
    })
}

#[cfg(feature = "libp2p")]
async fn handle_se_artifacts_received<S: storage::corekv::Store + 'static>(
    store: Arc<S>,
    event_bus: Arc<dyn events::Bus>,
    peer_id: String,
    data: Vec<u8>,
) {
    let mut txn = match store.new_txn(false).await {
        Ok(txn) => txn,
        Err(error) => {
            tracing::warn!(peer_id = %peer_id, error = %error, "failed to create SE artifact transaction");
            return;
        }
    };

    let result = match db::merge::se::receive_and_store(&mut txn, &data).await {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(peer_id = %peer_id, error = %error, "failed to receive SE artifacts");
            return;
        }
    };

    if let Err(error) = txn.commit().await {
        tracing::warn!(peer_id = %peer_id, error = %error, "failed to commit SE artifacts");
        return;
    }

    tracing::debug!(
        peer_id = %peer_id,
        collection_id = %result.collection_id,
        stored = result.stored,
        rejected = result.rejected,
        "stored incoming SE artifacts"
    );

    for doc_id in result.doc_ids {
        event_bus.publish(events::Message::se_artifact_received(
            events::SEArtifactReceivedData { doc_id },
        ));
    }
}

#[cfg(feature = "libp2p")]
pub(crate) fn spawn_replication_loop<B, T, S>(
    coordinator: Arc<p2p::sync::SyncCoordinator<B, T>>,
    sync_events_rx: tokio::sync::mpsc::Receiver<p2p::sync::SyncEvent>,
    merge_handler: Arc<db::merge::PooledMergeHandler<EmbeddedMergeHandler<S>>>,
    event_bus: Arc<dyn events::Bus>,
) -> tokio::task::JoinHandle<()>
where
    B: blockstore::Blockstore + 'static,
    T: p2p::P2PTransport,
    S: storage::corekv::Store + 'static,
{
    tokio::spawn(async move {
        let local_peer = coordinator.local_peer_id().to_string();
        ReplicationLoop::run(
            coordinator,
            sync_events_rx,
            merge_handler,
            ReplicationConfig::default(),
            move |result| {
                defra_p2p_adapter::publish_replication_result(
                    event_bus.as_ref(),
                    &local_peer,
                    result,
                )
            },
        )
        .await;
    })
}
