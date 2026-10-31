//! Command handlers for the iroh endpoint event loop.

use rapidhash::RapidHashMap;
use std::sync::Arc;

use bytes::Bytes;
use tracing::{debug, warn};

use crate::bitswap::ReplicatorRegistry;
use crate::sync::DispatchSender;
use crate::transport::{MessageId, PeerAddr, PeerId, TransportEvent};
use crate::QueryId;

use super::addr::{endpoint_addr_from_parts, endpoint_ticket_string};
use super::command::IrohCommand;
use super::endpoint::{peer_direct_addr, spawn_task, ActiveSync, EndpointResources, RawTopics};
use super::endpoint_rpc::{
    close_peer_connections, connect_mux, handle_block_sync, handle_car_request_response,
    handle_fire_and_forget, handle_reply_on_stream, handle_request_response,
    handle_two_stream_request, remember_connection, reply_on_stream, BlockSyncResources,
    REQUEST_RESPONSE_TIMEOUT,
};
use super::endpoint_streams::ConnectionStreamContext;
use super::gossip::{AddrInfo, EventSender, Network};
use super::peer_map::{endpoint_id_to_peer_id, parse_endpoint_id};
use super::protocols;
use super::records::Input as RecordsInput;

/// Handle a command from `IrohTransport`.
///
/// Returns `true` if the event loop should shut down.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_command(
    cmd: IrohCommand,
    resources: &EndpointResources,
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
        IrohCommand::WireStats { reply } => {
            let _ = reply.send(Ok(Arc::clone(&resources.wire_stats)));
        }
        IrohCommand::PeerConnection { peer_id, reply } => {
            let result = parse_endpoint_id(&peer_id).map(|id| peer_map.connection(&id));
            let _ = reply.send(result);
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
        IrohCommand::SetServiceProfile { profile, reply } => {
            resources
                .records
                .push_change(RecordsInput::Profile(profile));
            let _ = reply.send(Ok(()));
        }
        IrohCommand::ServiceRecord { peer_id, reply } => match parse_endpoint_id(&peer_id) {
            Ok(node) => resources
                .records
                .push_change(RecordsInput::Lookup { node, reply }),
            Err(error) => {
                let _ = reply.send(Err(error));
            }
        },
        IrohCommand::CollectionProviders {
            collection_id,
            reply,
        } => {
            let ask = peer_map
                .connected_peers()
                .iter()
                .filter_map(|peer| parse_endpoint_id(peer).ok())
                .collect();
            resources.records.push_change(RecordsInput::Providers {
                id: collection_id,
                ask,
                reply,
            });
        }
        IrohCommand::ReplicatorOffers {
            collection_id,
            reply,
        } => {
            resources.records.push_change(RecordsInput::Replicators {
                id: collection_id,
                reply,
            });
        }
        IrohCommand::Subscribe { topic, reply } => {
            let joined = gossip.subscribe(topic.to_string());
            held_collection(resources, &topic, true);
            let _ = reply.send(Ok(joined));
        }
        IrohCommand::Unsubscribe { topic, reply } => {
            let left = gossip.unsubscribe(topic.to_string());
            held_collection(resources, &topic, false);
            let _ = reply.send(Ok(left));
        }
        IrohCommand::Publish { topic, msg, reply } => {
            let result = match msg.encode_gossip_payload() {
                Ok(payload) => {
                    gossip.publish(topic.to_string(), Bytes::from(payload), false);
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
            let _ = reply.send(Ok(gossip.subscribe(topic)));
        }
        IrohCommand::PublishRaw { topic, data, reply } => {
            // A KMS reply: a broadcast before any neighbour joins is lost (#976).
            gossip.publish(topic, Bytes::from(data), true);
            let _ = reply.send(Ok(MessageId::new(uuid::Uuid::new_v4().to_string())));
        }
        IrohCommand::TopicPeers { topic, reply } => {
            let _ = reply.send(Ok(gossip.topic_peers(&topic.to_string())));
        }
        IrohCommand::ReplyOnStream {
            send_stream,
            payload,
            reply,
        } => {
            let _ = spawn_task(spawned_tasks, async move {
                let _ = reply
                    .send(reply_on_stream(send_stream, &payload, REQUEST_RESPONSE_TIMEOUT).await);
            });
        }
        IrohCommand::SendTwoStreamRequest {
            peer_id,
            request,
            reply,
        } => {
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let _ = spawn_task(spawned_tasks, async move {
                let result = handle_two_stream_request(
                    &endpoint,
                    &peer_id,
                    &request,
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
            // An older peer replies on a stream of its own.
            expect_reply(resources, &peer_id, protocols::STREAM_SE_QUERY_RESP);
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let event_tx = event_tx.clone();
            let _ = spawn_task(spawned_tasks, async move {
                handle_reply_on_stream(
                    &endpoint,
                    peer_id,
                    protocols::STREAM_SE_QUERY_REQ,
                    protocols::STREAM_SE_QUERY_RESP,
                    &request,
                    direct_addr,
                    &connection_cache,
                    &admission,
                    &event_tx,
                    reply,
                    |peer_id, reply| TransportEvent::SEQueryReply { peer_id, reply },
                )
                .await;
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
            // An older peer replies on a stream of its own.
            expect_reply(resources, &peer_id, protocols::STREAM_MANAGE_RESP);
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let event_tx = event_tx.clone();
            let _ = spawn_task(spawned_tasks, async move {
                handle_reply_on_stream(
                    &endpoint,
                    peer_id,
                    protocols::STREAM_MANAGE_REQ,
                    protocols::STREAM_MANAGE_RESP,
                    &request,
                    direct_addr,
                    &connection_cache,
                    &admission,
                    &event_tx,
                    reply,
                    |peer_id, reply| TransportEvent::ManageReply { peer_id, reply },
                )
                .await;
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
            // An older peer replies on a stream of its own.
            expect_reply(resources, &peer_id, protocols::STREAM_MANAGE_QUERY_RESP);
            let direct_addr = peer_direct_addr(peer_map, &peer_id);
            let endpoint = endpoint.clone();
            let connection_cache = Arc::clone(connection_cache);
            let admission = Arc::clone(&resources.admission);
            let event_tx = event_tx.clone();
            let _ = spawn_task(spawned_tasks, async move {
                handle_reply_on_stream(
                    &endpoint,
                    peer_id,
                    protocols::STREAM_MANAGE_QUERY_REQ,
                    protocols::STREAM_MANAGE_QUERY_RESP,
                    &request,
                    direct_addr,
                    &connection_cache,
                    &admission,
                    &event_tx,
                    reply,
                    |peer_id, reply| TransportEvent::ManageQueryReply { peer_id, reply },
                )
                .await;
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
    event_tx: EventSender,
}

/// Gossip's way out to the network: its dials take the same admission checks
/// as every other dial.
pub(super) struct GossipDialer(DialContext);

impl GossipDialer {
    pub(super) fn new(resources: &EndpointResources, event_tx: &EventSender) -> Self {
        Self(DialContext {
            resources: resources.clone(),
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
                gossip.dial_failed(peer);
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
    remember_connection(&ctx.resources.connection_cache, &connection);

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
        ctx.resources.gossip.connected(endpoint_id, &connection);
    }

    // Keep connection alive by spawning a handler for incoming streams.
    //
    // Spawned even when the re-check above refused the peer, and deliberately:
    // `increment_connections` has already counted this connection, and it is
    // this task's cleanup that decrements it. Returning early instead would
    // leave the count stuck above zero, which is worse than the race it was
    // meant to close: the revoked peer would sit in `connected_peers` forever,
    // looking connected when it holds nothing.
    let stream_context =
        ConnectionStreamContext::new(&ctx.resources, endpoint_id, ctx.event_tx.clone());
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

/// Tells the records task about a collection we start or stop holding, so
/// our record advertises it and its join is pointed at other holders.
fn held_collection(resources: &EndpointResources, topic: &crate::topics::DefraTopic, held: bool) {
    if let crate::topics::DefraTopic::Collection(id) = topic {
        resources.records.push_change(RecordsInput::Collection {
            id: id.clone(),
            held,
        });
    }
}

/// Expects `peer_id`'s reply on `reply_tag`, before the request goes out so
/// a fast reply finds it. An unparseable id fails the send itself.
fn expect_reply(resources: &EndpointResources, peer_id: &PeerId, reply_tag: &'static [u8]) {
    if let Ok(peer) = parse_endpoint_id(peer_id) {
        resources.solicited.expect(peer, reply_tag);
    }
}
