//! Handing an iroh endpoint's transport events to its coordinator.

use std::sync::Arc;

use p2p::iroh::IrohTransport;
use p2p::sync::{DispatchAdmission, IrohSyncCoordinator};
use p2p::{P2PTransport, TransportEvent};
use storage::corekv::Store;

use crate::manage::hooks::ManageHooksCell;

type Events =
    tokio::sync::mpsc::Receiver<TransportEvent<<IrohTransport as P2PTransport>::ResponseToken>>;

/// Dispatch every transport event. Peer lifecycle is published to the event
/// bus and re-arms retries; searchable-encryption and management traffic is
/// served here; everything else goes to the coordinator.
pub(super) fn spawn_event_handler<S, B>(
    events: Events,
    coordinator: Arc<IrohSyncCoordinator<B>>,
    store: Arc<S>,
    event_bus: Arc<dyn events::Bus>,
    transport: IrohTransport,
    manage_hooks: ManageHooksCell,
    #[cfg(not(target_arch = "wasm32"))] se_correlator: p2p::SeQueryCorrelator,
) -> n0_future::task::JoinHandle<()>
where
    S: Store + 'static,
    B: blockstore::Blockstore + 'static,
{
    n0_future::task::spawn(async move {
        let dispatcher = Arc::clone(&coordinator);
        dispatcher
            .run_event_dispatcher(events, move |event, admission| {
                let coordinator = Arc::clone(&coordinator);
                let store = Arc::clone(&store);
                let event_bus = Arc::clone(&event_bus);
                let transport = transport.clone();
                let manage_hooks = manage_hooks.clone();
                #[cfg(not(target_arch = "wasm32"))]
                let se_correlator = se_correlator.clone();
                async move {
                    publish_peer_lifecycle(&event, event_bus.as_ref(), &store).await;

                    if admission == DispatchAdmission::Saturated {
                        if let Err(error) = coordinator
                            .handle_transport_event_with_admission(event, admission)
                            .await
                        {
                            tracing::debug!(%error, "rejected saturated iroh request");
                        }
                        return;
                    }

                    let event = match event {
                        TransportEvent::SEArtifactsReceived { peer_id, data } => {
                            let doc_ids = db::merge::se::serve::handle_artifacts_received(
                                store.as_ref(),
                                &peer_id.to_string(),
                                &data,
                            )
                            .await;
                            for doc_id in doc_ids {
                                event_bus.publish(events::Message::se_artifact_received(
                                    events::SEArtifactReceivedData { doc_id },
                                ));
                            }
                            return;
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        TransportEvent::SEQueryRequest { peer_id, request } => {
                            db::merge::se::serve::handle_query_request(
                                store.as_ref(),
                                &transport,
                                peer_id,
                                request,
                            )
                            .await;
                            return;
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        TransportEvent::SEQueryReply { reply, .. } => {
                            se_correlator.deliver(reply);
                            return;
                        }
                        TransportEvent::ManageRequest { peer_id, request } => {
                            match manage_hooks.get() {
                                Some(hooks) => {
                                    crate::manage::serve::serve_manage_request(
                                        hooks, &transport, &peer_id, request,
                                    )
                                    .await
                                }
                                None => tracing::debug!(%peer_id, "manage request before hooks ready; dropping"),
                            }
                            return;
                        }
                        TransportEvent::ManageQueryRequest { peer_id, request } => {
                            match manage_hooks.get() {
                                Some(hooks) => {
                                    crate::manage::serve::serve_manage_query_request(
                                        hooks, &transport, &peer_id, request,
                                    )
                                    .await
                                }
                                None => tracing::debug!(%peer_id, "manage query request before hooks ready; dropping"),
                            }
                            return;
                        }
                        TransportEvent::ManageReply { reply, .. } => {
                            if let Some(hooks) = manage_hooks.get() {
                                hooks.correlator.deliver(reply);
                            }
                            return;
                        }
                        TransportEvent::ManageQueryReply { reply, .. } => {
                            if let Some(hooks) = manage_hooks.get() {
                                hooks.query_correlator.deliver(reply);
                            }
                            return;
                        }
                        other => other,
                    };

                    let event_kind = event.kind();
                    if let Err(error) = coordinator
                        .handle_transport_event_with_admission(event, admission)
                        .await
                    {
                        if error.is_rate_limited() {
                            tracing::debug!(event_kind, %error, "P2P rate-limited");
                        } else if error.is_retriable() {
                            tracing::warn!(event_kind, %error, "P2P transport event failed after retries");
                        } else {
                            tracing::error!(event_kind, %error, "P2P event handler error");
                        }
                    }
                }
            })
            .await;
    })
}

async fn publish_peer_lifecycle<T, S: Store>(
    event: &TransportEvent<T>,
    event_bus: &dyn events::Bus,
    store: &Arc<S>,
) {
    match event {
        TransportEvent::PeerConnected(peer_id) => {
            tracing::info!("Peer connected (iroh): {peer_id}");
            crate::activate_retry_peer(Arc::clone(store), peer_id).await;
        }
        TransportEvent::PeerDisconnected(peer_id) => {
            tracing::info!("Peer disconnected (iroh): {peer_id}");
        }
        TransportEvent::PeerSubscribed { peer_id, topic } => {
            publish_topic_peer(event_bus, peer_id, topic, "JOINED");
        }
        TransportEvent::PeerUnsubscribed { peer_id, topic } => {
            publish_topic_peer(event_bus, peer_id, topic, "LEFT");
        }
        _ => {}
    }
}

fn publish_topic_peer(
    event_bus: &dyn events::Bus,
    peer_id: &p2p::transport::PeerId,
    topic: &str,
    event_type: &str,
) {
    event_bus.publish(events::Message::topic_peer_event(
        events::TopicPeerEventData {
            peer_id: peer_id.to_string(),
            topic: topic.to_string(),
            event_type: event_type.to_string(),
        },
    ));
}
