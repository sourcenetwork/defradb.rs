//! Command handlers for the iroh endpoint event loop.

use rapidhash::RapidHashMap;
use std::sync::Arc;

use bytes::Bytes;
use kovan_queue::seg_queue::SegQueue;
use tokio::sync::oneshot;
use tracing::{debug, warn};

use crate::bitswap::ReplicatorRegistry;
use crate::sync::DispatchSender;
use crate::transport::{MessageId, PeerAddr, PeerId, TransportEvent};
use crate::QueryId;

use super::addr::{endpoint_addr_from_parts, endpoint_ticket_string};
use super::command::IrohCommand;
use super::endpoint::{
    peer_direct_addr, spawn_task, ActiveSync, EndpointResources, PendingPushLogReplies, RawTopics,
};
use super::endpoint_rpc::{
    close_peer_connections, connect_mux, handle_block_sync, handle_car_request_response,
    handle_fire_and_forget, handle_request_response, handle_send_only, handle_two_stream_request,
    remember_connection, BlockSyncResources,
};
use super::endpoint_streams::ConnectionStreamContext;
use super::gossip::{AddrInfo, EventSender, Network};
use super::peer_map::{endpoint_id_to_peer_id, parse_endpoint_id};
use super::protocols;

/// Handle a command from `IrohTransport`.
///
/// Returns `true` if the event loop should shut down.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_command(
    cmd: IrohCommand,
    resources: &EndpointResources,
    pending_pushlog_replies: &PendingPushLogReplies,
    raw_topics: &RawTopics,
    replicators: &Arc<ReplicatorRegistry>,
    active_syncs: &mut RapidHashMap<u64, ActiveSync>,
    next_query_id: &mut u64,
    event_tx: &DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
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
        IrohCommand::IsPeerRevoked { peer_id, reply } => {
            let result = parse_endpoint_id(&peer_id).map(|id| resources.admission.is_revoked(&id));
            let _ = reply.send(result);
        }
        IrohCommand::Listen { addr: _, reply } => {
            // iroh endpoint is already listening after bind
            let _ = reply.send(Ok(()));
        }
        IrohCommand::ConnectedPeers { reply } => {
            let _ = reply.send(Ok(peer_map.connected_peers()));
        }
        IrohCommand::InboundBudget { reply } => {
            let _ = reply.send(Ok(resources.inbound_budget.snapshot().to_vec()));
        }
        IrohCommand::FetchStats { reply } => {
            let _ = reply.send(Ok(resources.fetch_stats.snapshot()));
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
            let _ = reply.send(Ok(gossip.subscribe(topic.to_string()).await));
        }
        IrohCommand::Unsubscribe { topic, reply } => {
            let _ = reply.send(Ok(gossip.unsubscribe(topic.to_string()).await));
        }
        IrohCommand::Publish { topic, msg, reply } => {
            let result = match msg.encode_gossip_payload() {
                Ok(payload) => {
                    gossip
                        .publish(topic.to_string(), Bytes::from(payload))
                        .await;
                    Ok(MessageId::new(uuid::Uuid::new_v4().to_string()))
                }
                Err(error) => Err(crate::error::Error::CborSerialization(error.to_string())),
            };
            let _ = reply.send(result);
        }
        IrohCommand::RegisterRawTopic { topic, reply } => {
            raw_topics.insert_if_absent(topic, ());
            let _ = reply.send(Ok(()));
        }
        IrohCommand::SubscribeRaw { topic, reply } => {
            // Raw first, so the topic is never delivered as a decoded PushLog.
            raw_topics.insert_if_absent(topic.clone(), ());
            let _ = reply.send(Ok(gossip.subscribe(topic).await));
        }
        IrohCommand::PublishRaw { topic, data, reply } => {
            gossip.publish(topic, Bytes::from(data)).await;
            let _ = reply.send(Ok(MessageId::new(uuid::Uuid::new_v4().to_string())));
        }
        IrohCommand::TopicPeers { topic, reply } => {
            let _ = reply.send(Ok(gossip.topic_peers(&topic.to_string())));
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
            let fetch_stats = Arc::clone(&resources.fetch_stats);
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
                    &fetch_stats,
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
                Arc::clone(&resources.fetch_stats),
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
/// the loop free to accept.
#[derive(Clone)]
struct DialContext {
    resources: EndpointResources,
    pending_pushlog_replies: PendingPushLogReplies,
    event_tx: EventSender,
}

/// Gossip's way out to the network: its dials take the same admission checks
/// as every other dial.
pub(super) struct GossipDialer(DialContext);

impl GossipDialer {
    pub(super) fn new(
        resources: &EndpointResources,
        pending_pushlog_replies: &PendingPushLogReplies,
        event_tx: &EventSender,
    ) -> Self {
        Self(DialContext {
            resources: resources.clone(),
            pending_pushlog_replies: Arc::clone(pending_pushlog_replies),
            event_tx: event_tx.clone(),
        })
    }
}

impl Network for GossipDialer {
    fn dial(&self, peer: iroh::EndpointId, addrs: Vec<PeerAddr>) {
        let ctx = self.0.clone();
        let tasks = Arc::clone(&ctx.resources.spawned_tasks);
        let _ = spawn_task(&tasks, async move {
            let gossip = ctx.resources.gossip.clone();
            let peer_id = endpoint_id_to_peer_id(&peer);
            if let Err(error) = handle_dial(ctx, &peer_id, addrs).await {
                debug!(%peer_id, %error, "Gossip dial failed");
                gossip.dial_failed(peer).await;
            }
        });
    }

    fn local_addr(&self) -> AddrInfo {
        self.0.resources.endpoint.addr().into()
    }
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

    let connection = connect_mux(&ctx.resources.endpoint, endpoint_addr).await?;

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

    if admitted {
        ctx.resources
            .gossip
            .connected(endpoint_id, &connection)
            .await;
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
/// accept-initiated connections) and any cached outbound-send connections. iroh `Connection` clones
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
