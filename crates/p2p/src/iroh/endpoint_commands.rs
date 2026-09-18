//! Command handlers for the iroh endpoint event loop.

use rapidhash::fast::RandomState;
use rapidhash::RapidHashMap;
use std::sync::Arc;

use bytes::Bytes;
use iroh_gossip::net::Gossip;
use iroh_gossip::proto::TopicId;
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::{debug, warn};

use crate::bitswap::ReplicatorRegistry;
use crate::message::PushLogBroadcast;
use crate::transport::{MessageId, PeerAddr, PeerId, TransportEvent};
use crate::QueryId;

use super::addr::{endpoint_addr_from_parts, endpoint_ticket_string};
use super::command::IrohCommand;
use super::endpoint::{
    peer_direct_addr, snapshot_subscription_senders, spawn_task, ActiveSync, EndpointResources,
    Neighbors, PendingPushLogReplies, RawTopics, SpawnedTasks, SubscriptionSenders,
    TopicSubscription,
};
use super::endpoint_config::PeerAdmission;
use super::endpoint_rpc::{
    close_peer_connections, handle_block_sync, handle_car_request_response, handle_fire_and_forget,
    handle_request_response, handle_send_only, handle_two_stream_request, remember_connection,
    BlockSyncResources,
};
use super::endpoint_streams::ConnectionStreamContext;
use super::gossip_heal;
use super::peer_map::{endpoint_id_to_peer_id, parse_endpoint_id, SharedPeerMap};
use super::protocols;

/// Authenticate the endpoint that originated a PushLog gossip envelope.
///
/// Iroh's `delivered_from` authenticates only the last hop. New publishers
/// therefore sign the canonical envelope with their endpoint key. The signed
/// origin proves who emitted the hint. The connected hop is retained only as
/// ingress metadata: a gossip relay may not own the linked DAG. During a
/// rolling upgrade, an unsigned envelope is accepted only when Iroh proves it
/// arrived directly from its publisher; any payload `SourcePeerID` is ignored.
fn authenticate_pushlog_origin(
    broadcast: &mut PushLogBroadcast,
    delivered_from: &iroh::EndpointId,
    is_direct: bool,
) -> crate::error::Result<iroh::EndpointId> {
    match (
        broadcast.source_peer_id.as_deref(),
        broadcast.origin_signature.as_deref(),
    ) {
        (Some(claimed_source), Some(signature_bytes)) => {
            let origin: iroh::EndpointId =
                claimed_source
                    .parse()
                    .map_err(|error: iroh::KeyParsingError| {
                        crate::error::Error::InvalidPeerId(error.to_string())
                    })?;
            let signature = iroh::Signature::try_from(signature_bytes)
                .map_err(|_| crate::error::Error::InvalidSignature)?;
            let signing_bytes = broadcast.origin_signing_bytes()?;
            origin
                .verify(&signing_bytes, &signature)
                .map_err(|_| crate::error::Error::InvalidSignature)?;
            broadcast.authenticate_origin_peer(origin.to_string());
            broadcast.authenticate_source_peer(delivered_from.to_string());
            Ok(origin)
        }
        (_, Some(_)) => Err(crate::error::Error::InvalidSignature),
        (_, None) if is_direct => {
            let origin = *delivered_from;
            // Never retain an unsigned payload claim, even on the compatible
            // direct path. Iroh's authenticated delivery metadata is the
            // authority in this case.
            broadcast.source_peer_id = Some(origin.to_string());
            broadcast.authenticate_origin_peer(origin.to_string());
            broadcast.authenticate_source_peer(origin.to_string());
            Ok(origin)
        }
        (_, None) => Err(crate::error::Error::MissingSignature),
    }
}

/// Handle a command from `IrohTransport`.
///
/// Returns `true` if the event loop should shut down.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_command(
    cmd: IrohCommand,
    resources: &EndpointResources,
    pending_pushlog_replies: &PendingPushLogReplies,
    subscriptions: &mut RapidHashMap<String, TopicSubscription>,
    raw_topics: &RawTopics,
    replicators: &Arc<ReplicatorRegistry>,
    active_syncs: &mut RapidHashMap<u64, ActiveSync>,
    next_query_id: &mut u64,
    event_tx: &mpsc::Sender<TransportEvent<iroh::endpoint::SendStream>>,
) -> bool {
    let endpoint = &resources.endpoint;
    let gossip = &resources.gossip;
    let peer_map = &resources.peer_map;
    let connection_cache = &resources.connection_cache;
    let spawned_tasks = &resources.spawned_tasks;

    active_syncs.retain(|_, sync| !sync.abort_handle.is_finished());

    match cmd {
        IrohCommand::Dial {
            peer_id,
            addrs,
            reply,
        } => {
            // SPAWN the dial off the command loop. `handle_dial` blocks on
            // `endpoint.connect()` (up to the dial timeout); awaiting it inline
            // here would park the endpoint `select!` in this branch and stop it
            // polling `accept()`, so two peers dialing each other in-window can
            // never accept each other's inbound connection — a mutual-dial
            // deadlock. The reply is delivered from the spawned task, so callers
            // still get their result.
            let ctx = DialContext {
                resources: resources.clone(),
                pending_pushlog_replies: Arc::clone(pending_pushlog_replies),
                subscription_senders: snapshot_subscription_senders(subscriptions),
                event_tx: event_tx.clone(),
            };
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_dial(ctx, &peer_id, addrs).await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::Disconnect { peer_id, reply } => {
            let result = handle_disconnect(peer_id, resources);
            let _ = reply.send(result);
        }
        IrohCommand::AllowPeer {
            peer_id,
            authority,
            reply,
        } => {
            let result =
                parse_endpoint_id(&peer_id).and_then(|id| resources.admission.allow(id, authority));
            let _ = reply.send(result);
        }
        IrohCommand::DenyPeer { peer_id, reply } => {
            let result = handle_deny_peer(peer_id, resources);
            let _ = reply.send(result);
        }
        IrohCommand::Listen { addr: _, reply } => {
            // iroh endpoint is already listening after bind
            let _ = reply.send(Ok(()));
        }
        IrohCommand::ConnectedPeers { reply } => {
            let _ = reply.send(Ok(peer_map.connected_peers()));
        }
        IrohCommand::ListenAddresses { reply } => {
            let endpoint_addr = endpoint.addr();
            let mut addrs = vec![
                PeerAddr::new(format!("iroh://{}", endpoint.id())),
                PeerAddr::new(endpoint_ticket_string(&endpoint_addr)),
            ];
            for socket_addr in endpoint_addr.ip_addrs() {
                let addr = PeerAddr::new(socket_addr.to_string());
                if !addrs.contains(&addr) {
                    addrs.push(addr);
                }
            }
            let _ = reply.send(Ok(addrs));
        }
        IrohCommand::PeerAddresses { reply } => {
            let _ = reply.send(Ok(peer_map.peer_addresses()));
        }
        IrohCommand::NetworkChange { reply } => {
            endpoint.network_change().await;
            let _ = reply.send(Ok(()));
        }
        IrohCommand::ResolvePeerIdentity {
            peer_id,
            request,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_request_response(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_IDENTITY,
                    &request,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::Subscribe { topic, reply } => {
            let result = handle_subscribe(
                gossip,
                subscriptions,
                peer_map,
                &resources.admission,
                raw_topics,
                topic,
                event_tx,
            )
            .await;
            let _ = reply.send(result);
        }
        IrohCommand::Unsubscribe { topic, reply } => {
            let topic_str = topic.to_string();
            if let Some(sub) = subscriptions.remove(&topic_str) {
                sub.reader_task.abort();
                let _ = reply.send(Ok(true));
            } else {
                let _ = reply.send(Ok(false));
            }
        }
        IrohCommand::Publish { topic, msg, reply } => {
            let result = handle_publish(
                gossip,
                subscriptions,
                peer_map,
                &resources.admission,
                topic,
                msg,
                spawned_tasks,
            );
            let _ = reply.send(result);
        }
        IrohCommand::RegisterRawTopic { topic, reply } => {
            raw_topics.insert_if_absent(topic, ());
            let _ = reply.send(Ok(()));
        }
        IrohCommand::SubscribeRaw { topic, reply } => {
            // Mark as raw-routed first so the reader spawned below emits
            // GossipRawMessage (not a decoded PushLogBroadcast) for it, then
            // join the gossip mesh with a real reader task.
            raw_topics.insert_if_absent(topic.clone(), ());
            let result = subscribe_topic_str(
                gossip,
                subscriptions,
                peer_map,
                &resources.admission,
                raw_topics,
                topic,
                event_tx,
            )
            .await;
            let _ = reply.send(result);
        }
        IrohCommand::PublishRaw { topic, data, reply } => {
            let result = handle_publish_raw(
                gossip,
                subscriptions,
                peer_map,
                &resources.admission,
                topic,
                data,
                spawned_tasks,
            );
            let _ = reply.send(result);
        }
        IrohCommand::TopicPeers { topic, reply } => {
            let topic_str = topic.to_string();
            let peers = subscriptions
                .get(&topic_str)
                .map(|sub| {
                    sub.neighbors
                        .keys()
                        .map(|id| endpoint_id_to_peer_id(&id))
                        .collect()
                })
                .unwrap_or_default();
            let _ = reply.send(Ok(peers));
        }
        IrohCommand::SendPushLogResponse {
            mut send_stream,
            reply_msg,
            reply,
        } => {
            let _ = spawn_task(spawned_tasks, async move {
                let result = async {
                    protocols::write_message(&mut send_stream, &reply_msg).await?;
                    send_stream.finish().map_err(|e| {
                        crate::error::Error::Transport(format!("failed to finish stream: {}", e))
                    })?;
                    Ok(())
                }
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendTwoStreamRequest {
            peer_id,
            request,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let pending_pushlog_replies = pending_pushlog_replies.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let message_id = request.message_id.clone();
            let _ = spawn_task(spawned_tasks, async move {
                let request_peer_id = peer_id.clone();
                let request_message_id = message_id.clone();
                let result = async move {
                    let (reply_tx, reply_rx) = oneshot::channel();
                    let slot = SegQueue::new();
                    slot.push(reply_tx);
                    pending_pushlog_replies.insert(request_message_id.clone(), Arc::new(slot));

                    let result = handle_two_stream_request(
                        &endpoint,
                        &request_peer_id,
                        &request,
                        direct_addr,
                        &connection_cache,
                        reply_rx,
                        &admission,
                    )
                    .await;
                    if let Some(slot) = pending_pushlog_replies.remove(&request_message_id) {
                        drop(slot.pop());
                    }
                    result
                }
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendTwoStreamResponse {
            peer_id,
            reply_msg,
            reply,
        } => {
            // The reply path for a request that did not advertise same-stream
            // reply support.
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_send_only(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_TWOSTREAM_RESP,
                    &reply_msg,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendDocSyncRequest {
            peer_id,
            request,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let event_tx = event_tx.clone();
            let _ = spawn_task(spawned_tasks, async move {
                let result: crate::error::Result<crate::message::DocSyncReply> =
                    handle_request_response(
                        &endpoint,
                        &peer_id,
                        protocols::STREAM_DOCSYNC,
                        &request,
                        direct_addr,
                        &connection_cache,
                        &admission,
                    )
                    .await;
                match result {
                    Ok(doc_reply) => {
                        let _ = event_tx
                            .send(TransportEvent::DocSyncReply {
                                peer_id,
                                reply: doc_reply,
                            })
                            .await;
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            });
        }
        IrohCommand::SendBranchableSyncRequest {
            peer_id,
            request,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let event_tx = event_tx.clone();
            let _ = spawn_task(spawned_tasks, async move {
                let result: crate::error::Result<crate::message::BranchableSyncReply> =
                    handle_request_response(
                        &endpoint,
                        &peer_id,
                        protocols::STREAM_BRANCHABLE,
                        &request,
                        direct_addr,
                        &connection_cache,
                        &admission,
                    )
                    .await;
                match result {
                    Ok(br_reply) => {
                        let _ = event_tx
                            .send(TransportEvent::BranchableSyncReply {
                                peer_id,
                                reply: br_reply,
                            })
                            .await;
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            });
        }
        IrohCommand::SendDocSyncResponse {
            peer_id,
            reply_msg,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_fire_and_forget(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_DOCSYNC_RESP,
                    &reply_msg,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendBranchableSyncResponse {
            peer_id,
            reply_msg,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_fire_and_forget(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_BRANCHABLE_RESP,
                    &reply_msg,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendDocSyncResponseToken {
            mut send_stream,
            reply_msg,
            reply,
        } => {
            let _ = spawn_task(spawned_tasks, async move {
                let result = async {
                    protocols::write_message(&mut send_stream, &reply_msg).await?;
                    send_stream.finish().map_err(|e| {
                        crate::error::Error::Transport(format!("failed to finish stream: {}", e))
                    })?;
                    Ok(())
                }
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendBranchableSyncResponseToken {
            mut send_stream,
            reply_msg,
            reply,
        } => {
            let _ = spawn_task(spawned_tasks, async move {
                let result = async {
                    protocols::write_message(&mut send_stream, &reply_msg).await?;
                    send_stream.finish().map_err(|e| {
                        crate::error::Error::Transport(format!("failed to finish stream: {}", e))
                    })?;
                    Ok(())
                }
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendCarRequest {
            peer_id,
            root_cid,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let event_tx = event_tx.clone();
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_car_request_response(
                    &endpoint,
                    &peer_id,
                    root_cid,
                    direct_addr,
                    &connection_cache,
                    &event_tx,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendCarResponse {
            peer_id,
            car_data,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_fire_and_forget(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_CAR_RESP,
                    &car_data,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendSEArtifacts {
            peer_id,
            request,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_fire_and_forget(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_SE,
                    &request,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendSEQueryRequest {
            peer_id,
            request,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_fire_and_forget(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_SE_QUERY_REQ,
                    &request,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendSEQueryResponse {
            peer_id,
            reply_msg,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_fire_and_forget(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_SE_QUERY_RESP,
                    &reply_msg,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendManageRequest {
            peer_id,
            request,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_fire_and_forget(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_MANAGE_REQ,
                    &request,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendManageResponse {
            peer_id,
            reply_msg,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_fire_and_forget(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_MANAGE_RESP,
                    &reply_msg,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendManageQueryRequest {
            peer_id,
            request,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_fire_and_forget(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_MANAGE_QUERY_REQ,
                    &request,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SendManageQueryResponse {
            peer_id,
            reply_msg,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_fire_and_forget(
                    &endpoint,
                    &peer_id,
                    protocols::STREAM_MANAGE_QUERY_RESP,
                    &reply_msg,
                    direct_addr,
                    &connection_cache,
                    &admission,
                )
                .await;
                let _ = reply.send(result);
            });
        }
        IrohCommand::SyncBlocks {
            root,
            providers,
            missing,
            reply,
        } => {
            let query_id = QueryId(*next_query_id);
            *next_query_id += 1;

            let resources = BlockSyncResources::new(
                endpoint.clone(),
                Arc::clone(peer_map),
                Arc::clone(connection_cache),
                event_tx.clone(),
                Arc::clone(&resources.admission),
            );
            let task = spawn_task(spawned_tasks, async move {
                handle_block_sync(resources, query_id, root, providers, missing).await;
            });
            if let Some(abort_handle) = task {
                active_syncs.insert(query_id.0, ActiveSync { abort_handle });
                let _ = reply.send(Ok(query_id));
            }
        }
        IrohCommand::CancelSync { query_id, reply } => {
            if let Some(sync) = active_syncs.remove(&query_id.0) {
                sync.abort_handle.abort();
                let _ = reply.send(Ok(true));
            } else {
                let _ = reply.send(Ok(false));
            }
        }
        IrohCommand::CreateReplicator {
            peer_id,
            mut info,
            reply,
        } => {
            info.id = peer_id.to_string();
            replicators.set_replicator_info(info);
            let _ = reply.send(Ok(()));
        }
        IrohCommand::DeleteReplicator { peer_id, reply } => {
            replicators.remove_peer(peer_id.as_str());
            let _ = reply.send(Ok(()));
        }
        IrohCommand::ListReplicators { reply } => {
            let _ = reply.send(Ok(replicators.list_replicator_info()));
        }
        IrohCommand::GetReplicator { peer_id, reply } => {
            let _ = reply.send(Ok(replicators.get_replicator_info(peer_id.as_str())));
        }
        IrohCommand::RemoveReplicatorCollections {
            peer_id,
            collections,
            reply,
        } => {
            let _ = reply.send(Ok(
                replicators.remove_peer_collections(peer_id.as_str(), &collections)
            ));
        }
        IrohCommand::Shutdown { reply } => {
            tracing::warn!("Iroh endpoint received shutdown command");
            let _ = reply.send(Ok(()));
            return true;
        }
    }
    false
}

/// OWNED endpoint state for a single dial. Owned (not borrowed) so the dial can
/// run in a SPAWNED task off the endpoint command loop: `endpoint.connect()`
/// blocks for up to the dial timeout, and awaiting it inline in the command
/// branch of the endpoint `select!` starves the sibling `accept()` branch — so
/// two peers dialing each other in-window deadlock (neither accepts the other's
/// inbound connection). Spawning the dial (like `accept()` already spawns) keeps
/// the loop free to accept. `subscriptions` is captured as an owned senders
/// snapshot (the only previously-borrowed field).
struct DialContext {
    resources: EndpointResources,
    pending_pushlog_replies: PendingPushLogReplies,
    subscription_senders: SubscriptionSenders,
    event_tx: mpsc::Sender<TransportEvent<iroh::endpoint::SendStream>>,
}

/// Dial a peer by EndpointId.
///
/// Keeps the connection alive by spawning a stream handler task.
#[allow(clippy::too_many_arguments)]
async fn handle_dial(
    ctx: DialContext,
    peer_id: &PeerId,
    addrs: Vec<PeerAddr>,
) -> crate::error::Result<()> {
    let endpoint_id = parse_endpoint_id(peer_id)?;

    // A revoked peer must not be dialled. Refusing only its inbound
    // connections is not a revocation: this node dials on its own initiative
    // (the replicator reconnect sweep dials exactly the registered peers
    // missing from `connected_peers`, and cutting a peer's connection is what
    // marks it missing), so without this check a revoked peer is re-dialled
    // BY US within seconds and regains full stream service over the
    // connection we opened.
    if !ctx.resources.admission.admits_outbound(&endpoint_id) {
        return Err(crate::error::Error::Dial(format!(
            "refusing to dial {peer_id}: peer is revoked"
        )));
    }

    let mut endpoint_addr = endpoint_addr_from_parts(peer_id, &addrs)?;

    // Fix B (#511 reverse-edge dial): the advertised address may be
    // identity-only — an iroh ticket published before direct-addr discovery
    // completed carries no dialable direct addr. iroh rc.0 `connect()` requires
    // the addr to be present in the EndpointAddr (there is no persistent
    // address book to seed), so when no direct addr was supplied, reuse the
    // observed address learned from an existing inbound connection (e.g. a peer
    // that already dialed us during its network join). This lets a reverse mesh
    // edge dial back over the path the peer opened to us, instead of failing
    // "Address Lookup failed" under no-relay/no-discovery.
    if endpoint_addr.ip_addrs().next().is_none() {
        if let Some(observed) = peer_direct_addr(&ctx.resources.peer_map, peer_id) {
            endpoint_addr = endpoint_addr.with_ip_addr(observed);
        }
    }

    let direct_addresses: Vec<std::net::SocketAddr> = endpoint_addr.ip_addrs().copied().collect();

    let connection = ctx
        .resources
        .endpoint
        .connect(endpoint_addr, protocols::ALPN_MUX)
        .await
        .map_err(|e| crate::error::Error::Dial(e.to_string()))?;

    // Send over the connection we just opened instead of dialling a second one
    // on the first message.
    remember_connection(&ctx.resources.connection_cache, peer_id, &connection)?;

    let is_new = ctx.resources.peer_map.increment_connections(
        endpoint_id,
        direct_addresses.first().copied(),
        connection.clone(),
    );

    // Re-check AFTER registering. The dial above can run for many seconds, and
    // a revoke landing inside that window read `peer_map` and the connection
    // cache before this connection was in either, so it closed nothing.
    // Registering first and re-checking second means one of the two always
    // sees the other.
    let admitted = ctx.resources.admission.admits_outbound(&endpoint_id);
    if !admitted {
        close_peer_connections(
            &ctx.resources.peer_map,
            &ctx.resources.connection_cache,
            &endpoint_id,
        );
        connection.close(0u32.into(), b"peer revoked");
    }

    if admitted
        && is_new
        && ctx
            .event_tx
            .send(TransportEvent::PeerConnected(peer_id.clone()))
            .await
            .is_err()
    {
        warn!("Event channel closed, cannot emit PeerConnected");
    }

    if admitted && is_new {
        gossip_heal::spawn_peer_connected_heal(
            &ctx.resources,
            &ctx.subscription_senders,
            endpoint_id,
        );
    }

    // Keep connection alive by spawning a handler for incoming streams.
    //
    // Spawned even when the re-check above refused the peer, and deliberately:
    // `increment_connections` has already counted this connection, and it is
    // this task's cleanup that decrements it. Returning early instead would
    // leave the count stuck above zero, which is worse than the race it was
    // meant to close: the revoked peer would sit in `connected_peers` forever,
    // looking connected when it holds nothing.
    let stream_context = ConnectionStreamContext::new(
        &ctx.resources,
        Arc::clone(&ctx.pending_pushlog_replies),
        ctx.event_tx.clone(),
    );
    let _ = spawn_task(&ctx.resources.spawned_tasks, async move {
        super::endpoint_streams::handle_connection_streams(connection, endpoint_id, stream_context)
            .await;
    });

    if !admitted {
        return Err(crate::error::Error::Dial(format!(
            "dial to {peer_id} raced a revoke: connection closed"
        )));
    }

    Ok(())
}

/// Hang up the live connection to a peer.
///
/// Closes the connection handle retained in `peer_map` (covering dial- and
/// accept-initiated connections), any cached outbound-send connections, and
/// any gossip connection injected by the heal path. iroh `Connection` clones
/// share the underlying QUIC connection, so closing any handle tears down the
/// connection; the stream task then observes the `accept_bi` error, decrements
/// the count, and emits `PeerDisconnected`.
///
/// Idempotent: disconnecting an already-absent peer returns `Ok(())`.
fn handle_disconnect(peer_id: PeerId, resources: &EndpointResources) -> crate::error::Result<()> {
    let endpoint_id = parse_endpoint_id(&peer_id)?;
    close_peer_connections(
        &resources.peer_map,
        &resources.connection_cache,
        &endpoint_id,
    );
    if let Some(connection) = resources.healer.take_conn(&endpoint_id) {
        connection.close(0u32.into(), b"disconnect");
    }
    // Gossip connections this node accepted live in none of the above: they
    // are handed to the gossip layer before the peer ever reaches `peer_map`.
    for connection in resources.healer.take_accepted(&endpoint_id) {
        connection.close(0u32.into(), b"disconnect");
    }
    Ok(())
}

/// Bar a peer in both directions and hang up every connection it holds.
///
/// Order matters, and it is bar-then-close, never close-then-bar. Every
/// admission check reads the bar, so once it is recorded no accept and no
/// dial can bring the peer back; only then is it safe to close what is
/// already open. Closing first would leave a window in which a reconnect,
/// or this node's own reconnect sweep, re-establishes the peer while it is
/// still admissible.
///
/// Closing is not enough on its own either, which is why both halves exist:
/// a connection being established concurrently with this call was not yet
/// visible to `handle_disconnect` when it looked. The accept and dial paths
/// therefore re-check the bar after publishing their connection handle, so
/// that a connection racing this call is closed by whichever side observes
/// the other. See `endpoint_streams::handle_incoming` and `handle_dial`.
///
/// Unlike the old allowlist-only withdrawal this is meaningful under
/// `AcceptAll`: the bar is a separate set, so one peer can be revoked
/// without narrowing who else may connect.
fn handle_deny_peer(peer_id: PeerId, resources: &EndpointResources) -> crate::error::Result<()> {
    let endpoint_id = parse_endpoint_id(&peer_id)?;
    resources.admission.revoke(endpoint_id);
    handle_disconnect(peer_id, resources)
}

/// The connected peers that may be handed to gossip as mesh neighbours.
///
/// Filtered, because seeding gossip with a peer id is not passive: iroh-gossip
/// holds the raw endpoint and dials its own mesh, so a revoked peer named here
/// is a revoked peer this node asks gossip to go and connect to, over a path
/// neither `handle_dial` nor the accept check ever sees. `peer_map` alone is
/// not safe to use for this: `take_connections` deliberately leaves the count
/// entry behind for the stream tasks to clear, so a peer revoked a moment ago
/// is still listed there with no live handles.
///
/// One function rather than a filter repeated at each call site, so the
/// subscribe and publish paths cannot drift apart on who counts as a
/// neighbour.
fn admitted_neighbours(
    peer_map: &SharedPeerMap,
    admission: &PeerAdmission,
) -> Vec<iroh::EndpointId> {
    peer_map
        .endpoint_ids()
        .into_iter()
        .filter(|id| admission.admits_outbound(id))
        .collect()
}

/// Subscribe to a gossip topic.
///
/// Passes all currently connected peers as initial neighbors so gossip messages
/// are immediately deliverable. iroh-gossip requires explicit neighbors unlike
/// libp2p-gossipsub which discovers them automatically.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_subscribe(
    gossip: &Gossip,
    subscriptions: &mut RapidHashMap<String, TopicSubscription>,
    peer_map: &SharedPeerMap,
    admission: &PeerAdmission,
    raw_topics: &RawTopics,
    topic: crate::topics::DefraTopic,
    event_tx: &mpsc::Sender<TransportEvent<iroh::endpoint::SendStream>>,
) -> crate::error::Result<bool> {
    subscribe_topic_str(
        gossip,
        subscriptions,
        peer_map,
        admission,
        raw_topics,
        topic.to_string(),
        event_tx,
    )
    .await
}

/// Join the gossip mesh for an arbitrary topic string and spawn a reader task.
///
/// Shared by [`handle_subscribe`] (typed `DefraTopic`) and the `SubscribeRaw`
/// command (raw string sub-topics such as the KMS `encryption/<peer>/_response`
/// reply topic). The topic id is derived via [`topic_to_id`] from the same
/// string `handle_publish_raw` uses, so a subscriber and a publisher of the
/// same string topic meet on the same iroh-gossip `TopicId`. The reader emits
/// [`TransportEvent::GossipRawMessage`] for any topic present in `raw_topics`,
/// and a decoded [`TransportEvent::GossipMessage`] otherwise.
#[allow(clippy::too_many_arguments)]
pub(super) async fn subscribe_topic_str(
    gossip: &Gossip,
    subscriptions: &mut RapidHashMap<String, TopicSubscription>,
    peer_map: &SharedPeerMap,
    admission: &PeerAdmission,
    raw_topics: &RawTopics,
    topic_str: String,
    event_tx: &mpsc::Sender<TransportEvent<iroh::endpoint::SendStream>>,
) -> crate::error::Result<bool> {
    use futures::StreamExt;

    if subscriptions.contains_key(&topic_str) {
        return Ok(false);
    }

    let topic_id = topic_to_id(&topic_str);
    let initial_peers = admitted_neighbours(peer_map, admission);
    let gossip_topic = gossip
        .subscribe(topic_id, initial_peers)
        .await
        .map_err(|e| crate::error::Error::GossipSubSubscription(e.to_string()))?;

    let (sender, mut receiver) = gossip_topic.split();
    let neighbors: Neighbors = Arc::new(HopscotchMap::with_hasher(RandomState::default()));
    neighbors.extend(receiver.neighbors().map(|id| (id, ())));

    let event_tx = event_tx.clone();
    let topic_str_clone = topic_str.clone();
    let reader_neighbors = Arc::clone(&neighbors);
    let raw_topics_reader = Arc::clone(raw_topics);
    let reader_task = n0_future::task::spawn(async move {
        while let Some(result) = receiver.next().await {
            match result {
                Ok(event) => match event {
                    iroh_gossip::api::Event::Received(msg) => {
                        let sender_peer_id = endpoint_id_to_peer_id(&msg.delivered_from);
                        if raw_topics_reader.contains_key(&topic_str_clone) {
                            let msg_id = MessageId::new(uuid::Uuid::new_v4().to_string());
                            if event_tx
                                .send(TransportEvent::GossipRawMessage {
                                    propagation_source: sender_peer_id,
                                    message_id: msg_id,
                                    topic: topic_str_clone.clone(),
                                    data: msg.content.to_vec(),
                                })
                                .await
                                .is_err()
                            {
                                debug!("Event channel closed, stopping gossip reader");
                                break;
                            }
                            continue;
                        }
                        match crate::message::PushLogBroadcast::decode_gossip_payload(&msg.content)
                        {
                            Ok((mut broadcast, encoding)) => {
                                let origin = match authenticate_pushlog_origin(
                                    &mut broadcast,
                                    &msg.delivered_from,
                                    msg.scope.is_direct(),
                                ) {
                                    Ok(origin) => origin,
                                    Err(error) => {
                                        warn!(
                                            peer_id = %sender_peer_id,
                                            claimed_source_peer_id = ?broadcast.source_peer_id,
                                            topic = %topic_str_clone,
                                            is_direct = msg.scope.is_direct(),
                                            error = %error,
                                            "Dropping Iroh head hint with unauthenticated origin"
                                        );
                                        continue;
                                    }
                                };
                                debug!(
                                    peer_id = %sender_peer_id,
                                    origin_peer_id = %origin,
                                    is_direct = msg.scope.is_direct(),
                                    topic = %topic_str_clone,
                                    "Authenticated Iroh head-hint origin and ingress hop"
                                );
                                if encoding != crate::message::PushLogGossipPayloadEncoding::PostcardBroadcast {
                                    debug!(
                                        peer_id = %sender_peer_id,
                                        topic = %topic_str_clone,
                                        message_size = msg.content.len(),
                                        ?encoding,
                                        "Decoded Iroh gossip message via compatibility fallback"
                                    );
                                }
                                let msg_id = MessageId::new(uuid::Uuid::new_v4().to_string());
                                if event_tx
                                    .send(TransportEvent::GossipMessage {
                                        propagation_source: sender_peer_id,
                                        message_id: msg_id,
                                        topic: topic_str_clone.clone(),
                                        message: broadcast,
                                    })
                                    .await
                                    .is_err()
                                {
                                    debug!("Event channel closed, stopping gossip reader");
                                    break;
                                }
                            }
                            Err(e) => {
                                let payload_info =
                                    crate::message::PushLogBroadcast::inspect_gossip_payload(
                                        &msg.content,
                                    );
                                let sample = crate::sync::GossipDecodeFailureSample {
                                    transport: crate::sync::GossipTransport::Iroh,
                                    peer_id: sender_peer_id.to_string(),
                                    topic: topic_str_clone.clone(),
                                    message_size: msg.content.len(),
                                    error: e.clone(),
                                    payload_fingerprint: payload_info.payload_fingerprint,
                                    payload_shape_hint: payload_info.payload_shape_hint,
                                    occurrences: 0,
                                };
                                // Exponential-backoff sampling: warn on the
                                // 1st, 2nd, 4th, 8th... occurrence; remainder
                                // at debug. Counter is process-global and
                                // surfaced via SyncDiagnostics (issue #858).
                                let count = crate::sync::record_gossip_decode_failure_sample(
                                    sample.clone(),
                                );
                                if count == 1 || count.is_power_of_two() {
                                    warn!(
                                        peer_id = %sender_peer_id,
                                        topic = %topic_str_clone,
                                        message_size = msg.content.len(),
                                        total_failures = count,
                                        error = %e,
                                        payload_fingerprint = %sample.payload_fingerprint,
                                        payload_shape = %sample.payload_shape_hint,
                                        "Failed to decode Iroh gossip message as PushLogBroadcast or PushLogRequest"
                                    );
                                } else {
                                    debug!(
                                        peer_id = %sender_peer_id,
                                        topic = %topic_str_clone,
                                        message_size = msg.content.len(),
                                        total_failures = count,
                                        error = %e,
                                        payload_fingerprint = %sample.payload_fingerprint,
                                        payload_shape = %sample.payload_shape_hint,
                                        "Failed to decode Iroh gossip message"
                                    );
                                }
                            }
                        }
                    }
                    iroh_gossip::api::Event::NeighborUp(id) => {
                        reader_neighbors.insert_if_absent(id, ());
                        if event_tx
                            .send(TransportEvent::PeerSubscribed {
                                peer_id: endpoint_id_to_peer_id(&id),
                                topic: topic_str_clone.clone(),
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    iroh_gossip::api::Event::NeighborDown(id) => {
                        reader_neighbors.remove(&id);
                        if event_tx
                            .send(TransportEvent::PeerUnsubscribed {
                                peer_id: endpoint_id_to_peer_id(&id),
                                topic: topic_str_clone.clone(),
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    iroh_gossip::api::Event::Lagged => {
                        warn!(
                            topic = %topic_str_clone,
                            "Gossip lagged — some messages were missed"
                        );
                    }
                },
                Err(e) => {
                    debug!("Gossip receiver error: {}", e);
                    break;
                }
            }
        }
    });

    subscriptions.insert(
        topic_str,
        TopicSubscription {
            sender,
            reader_task,
            neighbors,
        },
    );
    Ok(true)
}

fn handle_publish(
    gossip: &Gossip,
    subscriptions: &RapidHashMap<String, TopicSubscription>,
    peer_map: &SharedPeerMap,
    admission: &PeerAdmission,
    topic: crate::topics::DefraTopic,
    msg: PushLogBroadcast,
    spawned_tasks: &SpawnedTasks,
) -> crate::error::Result<MessageId> {
    let topic_str = topic.to_string();
    let topic_id = topic_to_id(&topic_str);
    let sender = subscriptions.get(&topic_str).map(|sub| sub.sender.clone());
    let gossip = gossip.clone();
    let initial_peers = admitted_neighbours(peer_map, admission);
    let message_id = MessageId::new(uuid::Uuid::new_v4().to_string());
    let payload = msg
        .encode_gossip_payload()
        .map_err(|error| crate::error::Error::CborSerialization(error.to_string()))?;

    let _ = spawn_task(spawned_tasks, async move {
        let sender = if let Some(sender) = sender {
            sender
        } else {
            match gossip.subscribe(topic_id, initial_peers).await {
                Ok(topic) => {
                    let (sender, _receiver) = topic.split();
                    sender
                }
                Err(error) => {
                    warn!(
                        topic = %topic_str,
                        error = %error,
                        "Failed to create ephemeral Iroh gossip publisher"
                    );
                    return;
                }
            }
        };

        if let Err(error) = sender.broadcast(Bytes::from(payload)).await {
            warn!(
                topic = %topic_str,
                error = %error,
                "Failed to publish Iroh gossip message"
            );
        }
    });

    Ok(message_id)
}

/// Publish raw bytes on a gossip topic (no PushLogBroadcast encoding).
///
/// Used by the KMS pubsub transport. Mirrors `handle_publish` but broadcasts
/// `data` directly instead of encoding a `PushLogBroadcast`.
fn handle_publish_raw(
    gossip: &Gossip,
    subscriptions: &RapidHashMap<String, TopicSubscription>,
    peer_map: &SharedPeerMap,
    admission: &PeerAdmission,
    topic_str: String,
    data: Vec<u8>,
    spawned_tasks: &SpawnedTasks,
) -> crate::error::Result<MessageId> {
    let topic_id = topic_to_id(&topic_str);
    let sender = subscriptions.get(&topic_str).map(|sub| sub.sender.clone());
    let gossip = gossip.clone();
    let initial_peers = admitted_neighbours(peer_map, admission);
    let message_id = MessageId::new(uuid::Uuid::new_v4().to_string());

    let _ = spawn_task(spawned_tasks, async move {
        let sender = if let Some(sender) = sender {
            sender
        } else {
            // No persistent subscription for this topic (the responder side of a
            // KMS `_response` reply, which only ever publishes here). Join
            // ephemerally and WAIT for the mesh to graft to at least one
            // neighbor before broadcasting — iroh-gossip has no store-and-
            // forward, so a broadcast on a freshly-joined, ungrafted topic
            // reaches nobody and the reply is silently lost (#976).
            match gossip.subscribe(topic_id, initial_peers).await {
                Ok(mut topic) => {
                    if let Err(error) =
                        n0_future::time::timeout(RAW_PUBLISH_JOIN_TIMEOUT, topic.joined()).await
                    {
                        warn!(
                            topic = %topic_str,
                            error = %error,
                            "Timed out waiting for raw Iroh gossip mesh to graft; broadcasting anyway"
                        );
                    }
                    topic.split().0
                }
                Err(error) => {
                    warn!(
                        topic = %topic_str,
                        error = %error,
                        "Failed to create ephemeral Iroh gossip publisher (raw)"
                    );
                    return;
                }
            }
        };

        if let Err(error) = sender.broadcast(Bytes::from(data)).await {
            warn!(
                topic = %topic_str,
                error = %error,
                "Failed to publish raw Iroh gossip message"
            );
        }
    });

    Ok(message_id)
}

/// Upper bound on how long a raw publish waits for its freshly-joined gossip
/// topic to graft to a neighbor before broadcasting anyway. Bounds the KMS
/// reply path so a permanently-unreachable peer cannot wedge the publish task.
const RAW_PUBLISH_JOIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Hash a topic string to an iroh-gossip `TopicId`.
fn topic_to_id(topic: &str) -> TopicId {
    let hash = blake3::hash(topic.as_bytes());
    TopicId::from(*hash.as_bytes())
}

#[cfg(test)]
mod origin_tests {
    use super::*;

    fn broadcast() -> PushLogBroadcast {
        PushLogBroadcast::new(
            "doc".to_string(),
            Bytes::from_static(&[1, 2, 3]),
            "collection".to_string(),
            "creator".to_string(),
            Bytes::from_static(&[4, 5, 6]),
        )
    }

    #[test]
    fn relayed_origin_is_verified_and_hop_is_only_ingress_metadata() {
        let origin = iroh::SecretKey::generate();
        let relay = iroh::SecretKey::generate().public();
        let mut message = broadcast();
        message.source_peer_id = Some(origin.public().to_string());
        let bytes = message.origin_signing_bytes().unwrap();
        message.origin_signature = Some(origin.sign(&bytes).to_bytes().to_vec());

        let authenticated = authenticate_pushlog_origin(&mut message, &relay, false).unwrap();
        assert_eq!(authenticated, origin.public());
        assert_eq!(
            message.authenticated_origin_peer_id(),
            Some(origin.public().to_string().as_str())
        );
        assert_eq!(
            message.authenticated_source_peer_id(),
            Some(relay.to_string().as_str())
        );
    }

    #[test]
    fn forged_relayed_origin_is_rejected() {
        let claimed = iroh::SecretKey::generate();
        let forger = iroh::SecretKey::generate();
        let relay = iroh::SecretKey::generate().public();
        let mut message = broadcast();
        message.source_peer_id = Some(claimed.public().to_string());
        let bytes = message.origin_signing_bytes().unwrap();
        message.origin_signature = Some(forger.sign(&bytes).to_bytes().to_vec());

        assert!(matches!(
            authenticate_pushlog_origin(&mut message, &relay, false),
            Err(crate::error::Error::InvalidSignature)
        ));
        assert!(message.authenticated_source_peer_id().is_none());
        assert!(message.authenticated_origin_peer_id().is_none());
    }

    #[test]
    fn rolling_unsigned_compatibility_is_direct_only_and_ignores_claim() {
        let direct = iroh::SecretKey::generate().public();
        let relay = iroh::SecretKey::generate().public();
        let mut direct_message = broadcast();
        direct_message.source_peer_id = Some("forged".to_string());
        assert_eq!(
            authenticate_pushlog_origin(&mut direct_message, &direct, true).unwrap(),
            direct
        );
        assert_eq!(
            direct_message.authenticated_source_peer_id(),
            Some(direct.to_string().as_str())
        );
        assert_eq!(
            direct_message.authenticated_origin_peer_id(),
            Some(direct.to_string().as_str())
        );

        let mut relayed_message = broadcast();
        relayed_message.source_peer_id = Some("forged".to_string());
        assert!(matches!(
            authenticate_pushlog_origin(&mut relayed_message, &relay, false),
            Err(crate::error::Error::MissingSignature)
        ));
    }
}
