use super::*;

use crate::message::{IdentityRequest, IdentityResponse, IDENTITY_UNCONFIGURED_ERROR};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::endpoint::{EndpointResources, SpawnedTasks};
use super::super::endpoint_config::AllowlistState;
use super::super::endpoint_streams::{
    adopt_new_connection, handle_incoming, AdoptionOutcome, ConnectionDirection,
    ConnectionLifecycle,
};
use super::super::inbound_budget::InboundBudget;
use super::super::records::RecordsHandle;
use super::super::solicited::Solicited;
use super::super::task_registry::TaskRegistry;

#[test]
fn disconnect_closes_replacement_despite_stale_cleanup() {
    edge::testing::edge()
        .block_on(async {
            for alpn in [protocols::ALPN_MUX, protocols::ALPN_MUX_V1] {
                let endpoint = tests::localhost_endpoint(vec![]).await;
                let peer = tests::localhost_endpoint(vec![alpn.to_vec()]).await;
                let (old, accepted_old) =
                    tokio::join!(endpoint.connect(peer.addr(), alpn), async {
                        peer.accept().await.unwrap().await.unwrap()
                    },);
                let old = old.unwrap();
                let (replacement, accepted_replacement) =
                    tokio::join!(endpoint.connect(peer.addr(), alpn), async {
                        peer.accept().await.unwrap().await.unwrap()
                    },);
                let replacement = replacement.unwrap();
                let cache = new_unmanaged_connection_cache(GossipHandle::new());
                remember_connection(&cache, &replacement);
                old.close(0u32.into(), b"old path closed");
                let start = regolith::sync::Barrier::new(2);
                std::thread::scope(|scope| {
                    let cleanup = scope.spawn(|| {
                        futures::executor::block_on(start.wait());
                        evict_connection(&cache, peer.id(), &old);
                    });
                    futures::executor::block_on(start.wait());
                    close_cached_connections(&cache, &peer.id());
                    cleanup.join().unwrap();
                });
                assert!(replacement.close_reason().is_some());
                assert!(
                    cached_connection(&cache, &PeerId::new(peer.id().to_string()))
                        .unwrap()
                        .is_none()
                );
                drop((accepted_old, accepted_replacement));
                endpoint.close().await;
                peer.close().await;
            }
        })
        .unwrap();
}

#[test]
fn legacy_only_peer_reuses_and_refreshes_its_shared_rpc_path() {
    edge::testing::edge()
        .block_on(async {
            let endpoint = tests::localhost_endpoint(vec![]).await;
            let peer = tests::localhost_endpoint(vec![protocols::ALPN_MUX_V1.to_vec()]).await;
            let id = PeerId::new(peer.id().to_string());
            let direct = peer.addr().ip_addrs().next().copied();
            let cache = new_unmanaged_connection_cache(GossipHandle::new());
            let admission = PeerAdmission::new(AllowlistState::AcceptAll);
            let (first, accepted_first) = tokio::join!(
                connect_with_cache(&endpoint, &id, direct, &cache, &admission),
                async { peer.accept().await.unwrap().await.unwrap() },
            );
            let first = first.unwrap();
            assert_eq!(first.alpn(), protocols::ALPN_MUX_V1);
            let reused = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                connect_with_cache(&endpoint, &id, direct, &cache, &admission),
            )
            .await
            .expect("a cache hit must not require another accept")
            .unwrap();
            assert_eq!(first.stable_id(), reused.stable_id());

            let (replacement, accepted_replacement) = tokio::join!(
                endpoint.connect(peer.addr(), protocols::ALPN_MUX_V1),
                async { peer.accept().await.unwrap().await.unwrap() },
            );
            let replacement = replacement.unwrap();
            remember_connection(&cache, &replacement);
            evict_connection(&cache, peer.id(), &first);
            assert_eq!(
                cached_connection(&cache, &id).unwrap().unwrap().stable_id(),
                replacement.stable_id()
            );
            close_cached_connections(&cache, &peer.id());
            assert!(cached_connection(&cache, &id).unwrap().is_none());
            assert!(replacement.close_reason().is_some());
            drop((accepted_first, accepted_replacement));
            endpoint.close().await;
            peer.close().await;
        })
        .unwrap();
}

async fn peer_endpoint(secret: iroh::SecretKey) -> Endpoint {
    Endpoint::builder(iroh::endpoint::presets::Minimal)
        .secret_key(secret)
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![protocols::ALPN_MUX.to_vec()])
        .bind_addr("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
        .unwrap()
        .bind()
        .await
        .unwrap()
}

async fn reverse_rpc_uses_inbound(retain_old: bool) {
    let endpoint = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
    let gossip = GossipHandle::new();
    // The production shape: the cache adopts what it dials, so this fixture
    // exercises the same lifecycle the endpoint runs.
    let peer_map = Arc::new(SharedPeerMap::new());
    let spawned_tasks: SpawnedTasks = Arc::new(TaskRegistry::default());
    let admission = Arc::new(PeerAdmission::new(AllowlistState::AcceptAll));
    let inbound_budget: Arc<InboundBudget> = Arc::default();
    let records = RecordsHandle::new();
    let solicited = Arc::new(Solicited::new());
    let (events, mut received) = crate::sync::dispatch_queue(8);
    let lifecycle = ConnectionLifecycle::from_parts(
        events.clone(),
        Arc::clone(&peer_map),
        None,
        Arc::clone(&spawned_tasks),
        Arc::clone(&admission),
        gossip.clone(),
        Arc::clone(&inbound_budget),
        records.clone(),
        Arc::clone(&solicited),
    );
    let resources = EndpointResources {
        endpoint: endpoint.clone(),
        gossip: gossip.clone(),
        peer_map,
        connection_cache: new_adopting_connection_cache(gossip, lifecycle),
        spawned_tasks,
        node_identity: None,
        admission,
        inbound_budget,
        fetch_stats: Arc::default(),
        records,
        solicited,
    };
    let key = iroh::SecretKey::generate();
    let peer = PeerId::new(key.public().to_string());
    let old = if retain_old {
        let old_peer = peer_endpoint(key.clone()).await;
        let (outbound, inbound) = tokio::join!(
            endpoint.connect(old_peer.addr(), protocols::ALPN_MUX),
            async { old_peer.accept().await.unwrap().await.unwrap() },
        );
        let outbound = outbound.unwrap();
        remember_connection(&resources.connection_cache, &outbound);
        resources.peer_map.increment_connections(
            old_peer.id(),
            old_peer.addr().ip_addrs().next().copied(),
            outbound.clone(),
        );
        Some((old_peer, outbound, inbound))
    } else {
        None
    };
    let restarted = peer_endpoint(key).await;
    let (incoming, ()) = tokio::join!(
        restarted.connect(endpoint.addr(), protocols::ALPN_MUX),
        async {
            let incoming = endpoint.accept().await.unwrap();
            handle_incoming(incoming, &resources, &events).await;
        },
    );
    let incoming = incoming.unwrap();
    let cached = cached_connection(&resources.connection_cache, &peer)
        .unwrap()
        .expect("the authenticated inbound handle must be available to reverse RPC");
    if let Some((_, old, _)) = &old {
        assert_ne!(
            cached.stable_id(),
            old.stable_id(),
            "the replacement must supersede the old path"
        );
        assert!(
            old.close_reason().is_none(),
            "adoption must preserve other in-flight streams"
        );
    }
    let direct = restarted.addr().ip_addrs().next().copied().unwrap();
    let shared = connect_with_cache(
        &endpoint,
        &peer,
        Some(direct),
        &resources.connection_cache,
        &resources.admission,
    )
    .await
    .unwrap();
    assert_eq!(shared.stable_id(), cached.stable_id());
    let reply = tokio::spawn(async move {
        let (mut send, mut recv) = incoming.accept_bi().await.unwrap();
        let request = recv.read_to_end(32).await.unwrap();
        send.write_all(&request).await.unwrap();
        send.finish().unwrap();
        // The peer's last connection handle must outlive acknowledgement of its reply.
        assert!(send.stopped().await.unwrap().is_none());
    });
    let response = n0_future::time::timeout(std::time::Duration::from_secs(2), async {
        let (mut send, mut recv) = shared.open_bi().await.unwrap();
        send.write_all(b"reverse-rpc").await.unwrap();
        send.finish().unwrap();
        recv.read_to_end(32).await.unwrap()
    })
    .await
    .expect("reverse RPC must reach the accepted connection");
    assert_eq!(response, b"reverse-rpc");
    reply.await.unwrap();
    if !retain_old {
        n0_future::time::timeout(std::time::Duration::from_secs(2), async {
            while let Some(event) = received.recv().await {
                if matches!(event, TransportEvent::PeerDisconnected(ref id) if id == &peer) {
                    return;
                }
            }
            panic!("disconnect observation closed before the peer disconnected");
        })
        .await
        .expect("the accepted connection's closure must be observed");
        assert!(
            resources
                .connection_cache
                .connections
                .get(&restarted.id())
                .is_none(),
            "stream shutdown must retire its cache entry without another RPC lookup"
        );
    }
    for task in resources.spawned_tasks.close() {
        let _ = task.await;
    }
    endpoint.close().await;
    restarted.close().await;
    if let Some((old_peer, _, _)) = old {
        old_peer.close().await;
    }
}

#[test]
fn accepted_inbound_connection_serves_reverse_rpc_without_another_dial() {
    edge::testing::edge()
        .block_on(async {
            reverse_rpc_uses_inbound(false).await;
        })
        .unwrap();
}

#[test]
fn authenticated_reconnection_replaces_an_open_old_rpc_path() {
    edge::testing::edge()
        .block_on(async {
            reverse_rpc_uses_inbound(true).await;
        })
        .unwrap();
}

/// The managed state a real endpoint runs with, for a test that drives the
/// accept and RPC paths directly instead of through `spawn_endpoint`.
fn managed_resources(endpoint: Endpoint, gossip: GossipHandle) -> EndpointResources {
    let peer_map = Arc::new(SharedPeerMap::new());
    let spawned_tasks: SpawnedTasks = Arc::new(TaskRegistry::default());
    let admission = Arc::new(PeerAdmission::new(AllowlistState::AcceptAll));
    let inbound_budget: Arc<InboundBudget> = Arc::default();
    let records = RecordsHandle::new();
    let solicited = Arc::new(Solicited::new());
    let (events, _events_rx) = crate::sync::dispatch_queue(8);
    let lifecycle = ConnectionLifecycle::from_parts(
        events,
        Arc::clone(&peer_map),
        None,
        Arc::clone(&spawned_tasks),
        Arc::clone(&admission),
        gossip.clone(),
        Arc::clone(&inbound_budget),
        records.clone(),
        Arc::clone(&solicited),
    );
    EndpointResources {
        endpoint,
        gossip: gossip.clone(),
        peer_map,
        connection_cache: new_adopting_connection_cache(gossip, lifecycle),
        spawned_tasks,
        node_identity: None,
        admission,
        inbound_budget,
        fetch_stats: Arc::default(),
        records,
        solicited,
    }
}

/// Regression: a mux created only by `connect_with_cache` must serve reverse
/// RPC on that same connection.
///
/// The dialer never runs `handle_dial` and never accepts an inbound
/// connection, so its only path to the peer is the RPC cache-miss dial. The
/// receiver adopts the accepted connection through `handle_incoming` — the
/// managed accept path — and then sends a real service request back over it.
/// On the defect the dialer never `accept_bi`s the connection it dialled, so
/// that reverse request is never served. The forward request in the same test
/// proves both endpoints and the connection were ready before the reverse
/// await, so a failure here is the reverse await and nothing else.
#[test]
fn rpc_dialed_connection_serves_reverse_service_request() {
    edge::testing::edge()
        .block_on(async {
            let dialer = tests::localhost_endpoint(vec![]).await;
            let acceptor = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let dialer_peer = PeerId::new(dialer.id().to_string());
            let acceptor_peer = PeerId::new(acceptor.id().to_string());

            let dialer_resources = managed_resources(dialer.clone(), GossipHandle::new());
            let acceptor_resources = managed_resources(acceptor.clone(), GossipHandle::new());

            // Both sides run their managed accept loop. The dialer must never
            // see a connection: the reverse path has to reuse the dialled one.
            let dialer_accepts = Arc::new(AtomicUsize::new(0));
            let (dialer_events, _dialer_events_rx) = crate::sync::dispatch_queue(8);
            let dialer_accept_task = n0_future::task::spawn({
                let endpoint = dialer.clone();
                let resources = dialer_resources.clone();
                let accepts = Arc::clone(&dialer_accepts);
                async move {
                    while let Some(incoming) = endpoint.accept().await {
                        accepts.fetch_add(1, Ordering::SeqCst);
                        handle_incoming(incoming, &resources, &dialer_events).await;
                    }
                }
            });
            let acceptor_accepts = Arc::new(AtomicUsize::new(0));
            let (acceptor_events, _acceptor_events_rx) = crate::sync::dispatch_queue(8);
            let acceptor_accept_task = n0_future::task::spawn({
                let endpoint = acceptor.clone();
                let resources = acceptor_resources.clone();
                let accepts = Arc::clone(&acceptor_accepts);
                async move {
                    while let Some(incoming) = endpoint.accept().await {
                        accepts.fetch_add(1, Ordering::SeqCst);
                        handle_incoming(incoming, &resources, &acceptor_events).await;
                    }
                }
            });

            assert!(
                cached_connection(&dialer_resources.connection_cache, &acceptor_peer)
                    .unwrap()
                    .is_none(),
                "the first connection must come from an RPC cache miss"
            );

            // Forward: the dialer's only route to the peer is the RPC cache
            // miss. A real service request (identity) is served on it.
            let direct = acceptor.addr().ip_addrs().next().copied();
            let forward = n0_future::time::timeout(
                std::time::Duration::from_secs(2),
                handle_request_response::<_, IdentityResponse>(
                    &dialer,
                    &acceptor_peer,
                    protocols::STREAM_IDENTITY,
                    &IdentityRequest::new(dialer_peer.to_string()),
                    direct,
                    &dialer_resources.connection_cache,
                    &dialer_resources.admission,
                ),
            )
            .await
            .expect("forward RPC over the RPC-dialled connection")
            .unwrap();
            assert_eq!(
                forward.err_message.as_deref(),
                Some(IDENTITY_UNCONFIGURED_ERROR),
                "the acceptor must answer the forward request"
            );

            let dialled = cached_connection(&dialer_resources.connection_cache, &acceptor_peer)
                .unwrap()
                .expect("the RPC dial must cache the connection it created");
            let adopted = cached_connection(&acceptor_resources.connection_cache, &dialer_peer)
                .unwrap()
                .expect("the accept path must cache the connection it adopted");
            // `stable_id` is a local handle identity, so it is compared per
            // endpoint (before/after), never across the two endpoints.
            assert_eq!(
                dialled.remote_id(),
                acceptor.id(),
                "the dialer's cached handle must point at the acceptor"
            );
            assert_eq!(
                adopted.remote_id(),
                dialer.id(),
                "the acceptor's cached handle must point at the dialer"
            );

            // Reverse: the acceptor reuses the adopted connection to send a
            // real service request back. No manual `accept_bi` echo stands in
            // for the dialer's pump.
            let reverse = n0_future::time::timeout(
                std::time::Duration::from_secs(2),
                handle_request_response::<_, IdentityResponse>(
                    &acceptor,
                    &dialer_peer,
                    protocols::STREAM_IDENTITY,
                    &IdentityRequest::new(acceptor_peer.to_string()),
                    None,
                    &acceptor_resources.connection_cache,
                    &acceptor_resources.admission,
                ),
            )
            .await
            .expect("reverse RPC must be served on the RPC-dialled connection")
            .unwrap();
            assert_eq!(
                reverse.err_message.as_deref(),
                Some(IDENTITY_UNCONFIGURED_ERROR),
                "the dialer must answer the reverse request"
            );

            assert_eq!(
                dialer_accepts.load(Ordering::SeqCst),
                0,
                "the reverse path must not open a second connection"
            );
            assert_eq!(
                acceptor_accepts.load(Ordering::SeqCst),
                1,
                "the acceptor must accept exactly one connection"
            );
            assert_eq!(
                cached_connection(&dialer_resources.connection_cache, &acceptor_peer)
                    .unwrap()
                    .unwrap()
                    .stable_id(),
                dialled.stable_id(),
                "the reverse request must not replace the dialled connection"
            );
            assert_eq!(
                cached_connection(&acceptor_resources.connection_cache, &dialer_peer)
                    .unwrap()
                    .unwrap()
                    .stable_id(),
                adopted.stable_id(),
                "the reverse request must not replace the adopted connection"
            );

            dialer_accept_task.abort();
            acceptor_accept_task.abort();
            for task in dialer_resources.spawned_tasks.close() {
                let _ = task.await;
            }
            for task in acceptor_resources.spawned_tasks.close() {
                let _ = task.await;
            }
            dialer.close().await;
            acceptor.close().await;
        })
        .unwrap();
}

#[test]
fn legacy_mux_does_not_replace_the_modern_shared_rpc_handle() {
    edge::testing::edge()
        .block_on(async {
            let endpoint = tests::localhost_endpoint(vec![]).await;
            let peer = tests::localhost_endpoint(vec![
                protocols::ALPN_MUX.to_vec(),
                protocols::ALPN_MUX_V1.to_vec(),
            ])
            .await;
            let (modern, accepted_modern) =
                tokio::join!(endpoint.connect(peer.addr(), protocols::ALPN_MUX), async {
                    peer.accept().await.unwrap().await.unwrap()
                },);
            let modern = modern.unwrap();
            let cache = new_unmanaged_connection_cache(GossipHandle::new());
            remember_connection(&cache, &modern);
            let (legacy, accepted_legacy) = tokio::join!(
                endpoint.connect(peer.addr(), protocols::ALPN_MUX_V1),
                async { peer.accept().await.unwrap().await.unwrap() },
            );
            let legacy = legacy.unwrap();
            remember_connection(&cache, &legacy);
            let selected = cached_connection(&cache, &PeerId::new(peer.id().to_string()))
                .unwrap()
                .unwrap();
            assert_eq!(selected.stable_id(), modern.stable_id());
            assert_eq!(selected.alpn(), protocols::ALPN_MUX);
            assert!(legacy.close_reason().is_none());
            assert!(accepted_modern.close_reason().is_none());
            assert!(accepted_legacy.close_reason().is_none());
            close_cached_connections(&cache, &peer.id());
            assert!(modern.close_reason().is_some());
            assert!(legacy.close_reason().is_some());
            assert!(cache.connections.get(&peer.id()).is_none());
            assert!(cache.legacy_connections.get(&peer.id()).is_none());
            endpoint.close().await;
            peer.close().await;
        })
        .unwrap();
}

/// One lifecycle over shared stores, so a test can adopt through two different
/// task registries (one open, one closed) against the same peer map and cache.
fn lifecycle_with(
    peer_map: &Arc<SharedPeerMap>,
    spawned_tasks: &SpawnedTasks,
    admission: &Arc<PeerAdmission>,
    gossip: &GossipHandle,
    inbound_budget: &Arc<InboundBudget>,
    records: &RecordsHandle,
    solicited: &Arc<Solicited>,
) -> (
    ConnectionLifecycle,
    crate::sync::DispatchReceiver<TransportEvent<iroh::endpoint::SendStream>>,
) {
    let (lifecycle, _event_tx, received) = lifecycle_with_capacity::<8>(
        peer_map,
        spawned_tasks,
        admission,
        gossip,
        inbound_budget,
        records,
        solicited,
    );
    (lifecycle, received)
}

/// [`lifecycle_with`] with an explicit per-lane event capacity, so a test can
/// saturate one event lane on purpose. Returns the sender too, because the
/// lifecycle's own sender is private to its module.
///
/// The capacity is a const parameter rather than an argument: it is fixed per
/// test, and keeping it out of the signature leaves the helper at seven
/// arguments.
fn lifecycle_with_capacity<const CAPACITY: usize>(
    peer_map: &Arc<SharedPeerMap>,
    spawned_tasks: &SpawnedTasks,
    admission: &Arc<PeerAdmission>,
    gossip: &GossipHandle,
    inbound_budget: &Arc<InboundBudget>,
    records: &RecordsHandle,
    solicited: &Arc<Solicited>,
) -> (
    ConnectionLifecycle,
    crate::sync::DispatchSender<TransportEvent<iroh::endpoint::SendStream>>,
    crate::sync::DispatchReceiver<TransportEvent<iroh::endpoint::SendStream>>,
) {
    let (events, received) = crate::sync::dispatch_queue(CAPACITY);
    let lifecycle = ConnectionLifecycle::from_parts(
        events.clone(),
        Arc::clone(peer_map),
        None,
        Arc::clone(spawned_tasks),
        Arc::clone(admission),
        gossip.clone(),
        Arc::clone(inbound_budget),
        records.clone(),
        Arc::clone(solicited),
    );
    (lifecycle, events, received)
}

/// Poll a predicate until it holds, bounded by the suite's 2s budget. Used
/// where cleanup runs on a spawned pump rather than inline in the call.
async fn wait_until<F: FnMut() -> bool>(mut predicate: F, what: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !predicate() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::task::yield_now().await;
    }
}

/// A closed task registry refuses the pump, so adoption must close only the
/// handle it just took, retire its cache entry, and leave the peer-map count
/// balanced — without touching a healthy connection to another peer.
#[test]
fn closed_registry_refuses_adoption_and_balances_the_peer_map() {
    edge::testing::edge()
        .block_on(async {
            let local = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let healthy_peer = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let refused_peer = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;

            let peer_map = Arc::new(SharedPeerMap::new());
            let admission = Arc::new(PeerAdmission::new(AllowlistState::AcceptAll));
            let gossip = GossipHandle::new();
            let inbound_budget: Arc<InboundBudget> = Arc::default();
            let records = RecordsHandle::new();
            let solicited = Arc::new(Solicited::new());

            // A healthy peer adopted through an open registry first, so the
            // refusal below has something it must not disturb.
            let healthy_tasks: SpawnedTasks = Arc::new(TaskRegistry::default());
            let (healthy_lifecycle, _healthy_events) = lifecycle_with(
                &peer_map,
                &healthy_tasks,
                &admission,
                &gossip,
                &inbound_budget,
                &records,
                &solicited,
            );
            let cache = new_adopting_connection_cache(gossip.clone(), healthy_lifecycle.clone());

            let (healthy_outbound, healthy_connection) = tokio::join!(
                healthy_peer.connect(local.addr(), protocols::ALPN_MUX),
                async {
                    local
                        .accept()
                        .await
                        .unwrap()
                        .accept()
                        .unwrap()
                        .await
                        .unwrap()
                },
            );
            let healthy_outbound = healthy_outbound.unwrap();
            assert_eq!(
                adopt_new_connection(
                    &healthy_lifecycle,
                    &cache,
                    healthy_connection.clone(),
                    healthy_peer.id(),
                    None,
                    ConnectionDirection::Inbound,
                )
                .await,
                AdoptionOutcome::Admitted,
                "an open registry must adopt the healthy peer"
            );
            assert_eq!(
                peer_map
                    .connection(&healthy_peer.id())
                    .expect("the healthy peer must be counted")
                    .open_connections,
                1
            );

            // Now a registry that has already shut down.
            let closed_tasks: SpawnedTasks = Arc::new(TaskRegistry::default());
            let (closed_lifecycle, _closed_events) = lifecycle_with(
                &peer_map,
                &closed_tasks,
                &admission,
                &gossip,
                &inbound_budget,
                &records,
                &solicited,
            );
            for task in closed_tasks.close() {
                let _ = task.await;
            }

            let (_refused_outbound, refused_connection) = tokio::join!(
                refused_peer.connect(local.addr(), protocols::ALPN_MUX),
                async {
                    local
                        .accept()
                        .await
                        .unwrap()
                        .accept()
                        .unwrap()
                        .await
                        .unwrap()
                },
            );
            let outcome = adopt_new_connection(
                &closed_lifecycle,
                &cache,
                refused_connection.clone(),
                refused_peer.id(),
                None,
                ConnectionDirection::Inbound,
            )
            .await;

            assert_eq!(
                outcome,
                AdoptionOutcome::ShuttingDown,
                "a closed registry is a shutdown, not a revoke"
            );
            assert!(
                refused_connection.close_reason().is_some(),
                "the refused connection's own handle must be closed"
            );
            assert!(
                cache.connections.get(&refused_peer.id()).is_none(),
                "a refused adoption must leave no cache entry"
            );
            assert!(
                peer_map.connection(&refused_peer.id()).is_none(),
                "a refused adoption must leave no peer-map count"
            );
            assert!(
                !peer_map
                    .connected_peers()
                    .contains(&PeerId::new(refused_peer.id().to_string())),
                "a refused peer must not look connected"
            );

            // The healthy connection is untouched: not closed, still counted,
            // still cached.
            assert!(
                healthy_connection.close_reason().is_none(),
                "refusing one peer must not close another peer's connection"
            );
            assert!(
                healthy_outbound.close_reason().is_none(),
                "the healthy peer's own handle must stay open"
            );
            assert_eq!(
                peer_map
                    .connection(&healthy_peer.id())
                    .expect("the healthy peer must still be counted")
                    .open_connections,
                1
            );
            assert!(
                cache.connections.get(&healthy_peer.id()).is_some(),
                "the healthy peer's cache entry must survive"
            );

            for task in healthy_tasks.close() {
                let _ = task.await;
            }
            local.close().await;
            healthy_peer.close().await;
            refused_peer.close().await;
        })
        .unwrap();
}

/// A peer revoked before its connection is adopted must end with no usable
/// handle, no cache entry and no peer-map count, and must not be announced.
#[test]
fn revoked_adoption_leaves_no_handle_and_no_cache_entry() {
    edge::testing::edge()
        .block_on(async {
            let local = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let peer = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;

            let peer_map = Arc::new(SharedPeerMap::new());
            let admission = Arc::new(PeerAdmission::new(AllowlistState::AcceptAll));
            let gossip = GossipHandle::new();
            let inbound_budget: Arc<InboundBudget> = Arc::default();
            let records = RecordsHandle::new();
            let solicited = Arc::new(Solicited::new());
            let spawned_tasks: SpawnedTasks = Arc::new(TaskRegistry::default());
            let (lifecycle, mut events) = lifecycle_with(
                &peer_map,
                &spawned_tasks,
                &admission,
                &gossip,
                &inbound_budget,
                &records,
                &solicited,
            );
            let cache = new_adopting_connection_cache(gossip.clone(), lifecycle.clone());

            // Revoked before the connection exists, so adoption is the step
            // that must refuse it.
            assert!(admission.revoke(peer.id()));

            let (outbound, connection) =
                tokio::join!(peer.connect(local.addr(), protocols::ALPN_MUX), async {
                    local
                        .accept()
                        .await
                        .unwrap()
                        .accept()
                        .unwrap()
                        .await
                        .unwrap()
                },);
            let outbound = outbound.unwrap();
            let outcome = adopt_new_connection(
                &lifecycle,
                &cache,
                connection.clone(),
                peer.id(),
                None,
                ConnectionDirection::Inbound,
            )
            .await;

            assert_eq!(
                outcome,
                AdoptionOutcome::Refused,
                "a revoked peer must not be adopted"
            );
            assert!(
                connection.close_reason().is_some(),
                "the revoked connection must be closed"
            );
            assert!(
                cache.connections.get(&peer.id()).is_none(),
                "a revoked adoption must leave no cache entry"
            );
            // The refused connection is still pumped, and the pump's exit path
            // is what decrements the count, so the peer map settles a moment
            // after adoption returns.
            wait_until(
                || peer_map.connection(&peer.id()).is_none(),
                "the revoked peer's peer-map count to reach zero",
            )
            .await;
            wait_until(
                || peer_map.connected_peers().is_empty(),
                "the revoked peer to leave connected_peers",
            )
            .await;

            // The pump's cleanup may announce PeerDisconnected; what must never
            // appear is a PeerConnected for the revoked peer.
            let mut announced = Vec::new();
            while let Ok(Some(event)) =
                n0_future::time::timeout(std::time::Duration::from_secs(2), events.recv()).await
            {
                announced.push(event);
            }
            assert!(
                !announced
                    .iter()
                    .any(|event| matches!(event, TransportEvent::PeerConnected(_))),
                "a revoked peer must not be announced as connected: {announced:?}"
            );

            for task in spawned_tasks.close() {
                let _ = task.await;
            }
            local.close().await;
            peer.close().await;
            drop(outbound);
        })
        .unwrap();
}

/// A cancelled adopt must not leave a counted, cached connection with no pump.
///
/// The completion event lane is filled first, so `adopt_new_connection` publishes
/// the connection (peer-map count and cache entry) and then parks on its
/// `PeerConnected` send. Dropping the adopt future there is exactly what a
/// command-loop shutdown does. The pump must already be registered, so closing
/// the connection lets it run its exit path: cache entry evicted, count back to
/// zero, and the peer announced gone. The queue is drained while that happens,
/// because the pump's own `PeerDisconnected` send would otherwise block on the
/// lane this test filled.
#[test]
fn cancelled_adopt_leaves_no_cached_connection_and_no_count() {
    edge::testing::edge()
        .block_on(async {
            let local = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let peer = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let peer_id = PeerId::new(peer.id().to_string());

            let peer_map = Arc::new(SharedPeerMap::new());
            let admission = Arc::new(PeerAdmission::new(AllowlistState::AcceptAll));
            let gossip = GossipHandle::new();
            let inbound_budget: Arc<InboundBudget> = Arc::default();
            let records = RecordsHandle::new();
            let solicited = Arc::new(Solicited::new());
            let spawned_tasks: SpawnedTasks = Arc::new(TaskRegistry::default());
            // One slot per lane, so a single event fills the completion lane.
            let (lifecycle, event_tx, mut events) = lifecycle_with_capacity::<1>(
                &peer_map,
                &spawned_tasks,
                &admission,
                &gossip,
                &inbound_budget,
                &records,
                &solicited,
            );
            let cache = new_adopting_connection_cache(gossip.clone(), lifecycle.clone());

            // Fill the completion lane with a known event. PeerConnected is a
            // Completion-class event, so this is the lane the adopt parks on.
            event_tx
                .send(TransportEvent::PeerConnected(PeerId::new(
                    "filler-peer".to_string(),
                )))
                .await
                .expect("the filler event must be queued");
            assert!(
                events.is_full(crate::sync::DispatchClass::Completion),
                "the completion lane must be full before the adopt"
            );

            let (outbound, connection) =
                tokio::join!(peer.connect(local.addr(), protocols::ALPN_MUX), async {
                    local
                        .accept()
                        .await
                        .unwrap()
                        .accept()
                        .unwrap()
                        .await
                        .unwrap()
                },);
            let outbound = outbound.unwrap();

            // Poll the adopt once: it publishes, then parks on the full lane.
            let mut adopt = Box::pin(adopt_new_connection(
                &lifecycle,
                &cache,
                connection.clone(),
                peer.id(),
                None,
                ConnectionDirection::Inbound,
            ));
            std::future::poll_fn(|cx| {
                assert!(
                    std::future::Future::poll(adopt.as_mut(), cx).is_pending(),
                    "the adopt must park on the full completion lane"
                );
                std::task::Poll::Ready(())
            })
            .await;
            assert!(
                cache.connections.get(&peer.id()).is_some(),
                "the adopt must have published its cache entry before parking"
            );
            assert_eq!(
                peer_map
                    .connection(&peer.id())
                    .expect("the adopt must have published its peer-map count")
                    .open_connections,
                1
            );

            // Cancel it, as a shutdown would.
            drop(adopt);

            // The pump must already be registered, so closing the connection
            // runs its exit path. Drain the lane meanwhile: the pump's own
            // PeerDisconnected send would otherwise block on it.
            let cleanup = async {
                connection.close(0u32.into(), b"adopt cancelled");
                wait_until(
                    || cache.connections.get(&peer.id()).is_none(),
                    "the cancelled adopt's cache entry to be evicted",
                )
                .await;
                wait_until(
                    || peer_map.connection(&peer.id()).is_none(),
                    "the cancelled adopt's peer-map count to reach zero",
                )
                .await;
            };
            let drain = async {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                loop {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    match n0_future::time::timeout(remaining, events.recv()).await {
                        Ok(Some(TransportEvent::PeerDisconnected(gone))) if gone == peer_id => {
                            break
                        }
                        Ok(Some(_)) => continue,
                        Ok(None) | Err(_) => break,
                    }
                }
            };
            let ((), ()) = tokio::join!(cleanup, drain);

            assert!(
                cache.connections.get(&peer.id()).is_none(),
                "a cancelled adopt must leave no cache entry"
            );
            assert!(
                peer_map.connection(&peer.id()).is_none(),
                "a cancelled adopt must leave no peer-map count"
            );
            assert!(
                !peer_map.connected_peers().contains(&peer_id),
                "a cancelled adopt must not leave the peer looking connected"
            );
            assert!(
                connection.close_reason().is_some(),
                "the cancelled adopt's own connection must be closed"
            );
            wait_until(
                || outbound.close_reason().is_some(),
                "the peer to observe the closed connection",
            )
            .await;

            for task in spawned_tasks.close() {
                let _ = task.await;
            }
            local.close().await;
            peer.close().await;
        })
        .unwrap();
}

#[test]
fn repeated_cache_hits_reuse_one_adopted_connection() {
    edge::testing::edge()
        .block_on(async {
            let local = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let peer = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let peer_id = PeerId::new(peer.id().to_string());
            let direct = peer.addr().ip_addrs().next().copied();

            let peer_map = Arc::new(SharedPeerMap::new());
            let admission = Arc::new(PeerAdmission::new(AllowlistState::AcceptAll));
            let gossip = GossipHandle::new();
            let inbound_budget: Arc<InboundBudget> = Arc::default();
            let records = RecordsHandle::new();
            let solicited = Arc::new(Solicited::new());
            let spawned_tasks: SpawnedTasks = Arc::new(TaskRegistry::default());
            let (lifecycle, _events) = lifecycle_with(
                &peer_map,
                &spawned_tasks,
                &admission,
                &gossip,
                &inbound_budget,
                &records,
                &solicited,
            );
            let cache = new_adopting_connection_cache(gossip.clone(), lifecycle);

            // The first connection is an RPC cache miss, adopted by the cache.
            let (first, accepted) = tokio::join!(
                connect_with_cache(&local, &peer_id, direct, &cache, &admission),
                async {
                    peer.accept()
                        .await
                        .unwrap()
                        .accept()
                        .unwrap()
                        .await
                        .unwrap()
                },
            );
            let first = first.unwrap();
            assert_eq!(
                peer_map
                    .connection(&peer.id())
                    .expect("the dialled peer must be counted")
                    .open_connections,
                1
            );

            for _ in 0..3 {
                let again = n0_future::time::timeout(
                    std::time::Duration::from_secs(2),
                    connect_with_cache(&local, &peer_id, direct, &cache, &admission),
                )
                .await
                .expect("a cache hit must not dial")
                .unwrap();
                assert_eq!(
                    again.stable_id(),
                    first.stable_id(),
                    "a cache hit must return the adopted connection"
                );
            }

            assert_eq!(
                peer_map
                    .connection(&peer.id())
                    .expect("the peer must still be counted")
                    .open_connections,
                1,
                "repeated cache hits must not increment the peer-map count"
            );
            assert_eq!(
                peer_map.connected_peers().len(),
                1,
                "repeated cache hits must not add a second peer entry"
            );
            assert!(
                n0_future::time::timeout(std::time::Duration::from_secs(2), peer.accept())
                    .await
                    .is_err(),
                "repeated cache hits must not open a second connection"
            );

            for task in spawned_tasks.close() {
                let _ = task.await;
            }
            local.close().await;
            peer.close().await;
            drop(accepted);
        })
        .unwrap();
}

/// A post-publication allowlist miss must cost only the connection it refused.
///
/// The allowlist governs who may open a connection TO us, so a peer that is
/// dropped from it keeps the outbound session this node dialled. The refused
/// inbound adopt publishes into the cache and displaces that sibling, so the
/// refusal has to put it back: the sibling must stay open, stay cached, keep
/// its peer-map count, and still serve RPC.
#[test]
fn allowlist_miss_refuses_only_the_new_connection_and_spares_the_sibling() {
    edge::testing::edge()
        .block_on(async {
            let local = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let peer = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let peer_id = PeerId::new(peer.id().to_string());
            let direct = peer.addr().ip_addrs().next().copied();

            // An explicit allowlist that does not contain the peer: inbound is
            // refused, outbound is not, and nothing is revoked.
            let peer_map = Arc::new(SharedPeerMap::new());
            let admission = Arc::new(PeerAdmission::new(AllowlistState::Explicit(
                HopscotchMap::with_hasher(RandomState::default()),
            )));
            let gossip = GossipHandle::new();
            let inbound_budget: Arc<InboundBudget> = Arc::default();
            let records = RecordsHandle::new();
            let solicited = Arc::new(Solicited::new());
            let spawned_tasks: SpawnedTasks = Arc::new(TaskRegistry::default());
            let (lifecycle, _events) = lifecycle_with(
                &peer_map,
                &spawned_tasks,
                &admission,
                &gossip,
                &inbound_budget,
                &records,
                &solicited,
            );
            let cache = new_adopting_connection_cache(gossip.clone(), lifecycle.clone());

            // The healthy sibling: an outbound session this node dialled, which
            // the allowlist does not govern.
            let (sibling, sibling_accepted) =
                tokio::join!(local.connect(peer.addr(), protocols::ALPN_MUX), async {
                    peer.accept()
                        .await
                        .unwrap()
                        .accept()
                        .unwrap()
                        .await
                        .unwrap()
                },);
            let sibling = sibling.unwrap();
            assert_eq!(
                adopt_new_connection(
                    &lifecycle,
                    &cache,
                    sibling.clone(),
                    peer.id(),
                    direct,
                    ConnectionDirection::Outbound,
                )
                .await,
                AdoptionOutcome::Admitted,
                "the allowlist must not bar an outbound dial"
            );
            assert_eq!(
                peer_map
                    .connection(&peer.id())
                    .expect("the dialled peer must be counted")
                    .open_connections,
                1
            );

            // Now the peer dials in, and the allowlist refuses it after the
            // publish.
            let (refused_outbound, refused) =
                tokio::join!(peer.connect(local.addr(), protocols::ALPN_MUX), async {
                    local
                        .accept()
                        .await
                        .unwrap()
                        .accept()
                        .unwrap()
                        .await
                        .unwrap()
                },);
            let refused_outbound = refused_outbound.unwrap();
            assert_eq!(
                adopt_new_connection(
                    &lifecycle,
                    &cache,
                    refused.clone(),
                    peer.id(),
                    None,
                    ConnectionDirection::Inbound,
                )
                .await,
                AdoptionOutcome::Refused,
                "an allowlist miss must refuse the inbound connection"
            );
            assert!(
                refused.close_reason().is_some(),
                "the refused connection's own handle must be closed"
            );

            // The refused connection is still pumped, so its cleanup settles a
            // moment after adoption returns.
            wait_until(
                || {
                    peer_map
                        .connection(&peer.id())
                        .is_some_and(|info| info.open_connections == 1)
                },
                "the refused connection's count to be released",
            )
            .await;

            // The sibling is untouched: open, cached, counted, and still the
            // connection RPC reaches the peer over.
            assert!(
                sibling.close_reason().is_none(),
                "an allowlist miss must not close the outbound sibling"
            );
            assert!(
                sibling_accepted.close_reason().is_none(),
                "the peer's own handle on the sibling must stay open"
            );
            assert_eq!(
                cache
                    .connections
                    .get(&peer.id())
                    .expect("the sibling must stay cached")
                    .stable_id(),
                sibling.stable_id(),
                "the refusal must put the displaced sibling back"
            );
            assert_eq!(
                peer_map
                    .connection(&peer.id())
                    .expect("the sibling must stay counted")
                    .open_connections,
                1,
                "the refusal must leave exactly the sibling's count"
            );

            let shared = n0_future::time::timeout(
                std::time::Duration::from_secs(2),
                connect_with_cache(&local, &peer_id, direct, &cache, &admission),
            )
            .await
            .expect("the sibling must still be reachable")
            .unwrap();
            assert_eq!(
                shared.stable_id(),
                sibling.stable_id(),
                "RPC must still use the sibling connection"
            );
            let reply = tokio::spawn(async move {
                let (mut send, mut recv) = sibling_accepted.accept_bi().await.unwrap();
                let request = recv.read_to_end(32).await.unwrap();
                send.write_all(&request).await.unwrap();
                send.finish().unwrap();
            });
            let response = n0_future::time::timeout(std::time::Duration::from_secs(2), async {
                let (mut send, mut recv) = shared.open_bi().await.unwrap();
                send.write_all(b"sibling-rpc").await.unwrap();
                send.finish().unwrap();
                recv.read_to_end(32).await.unwrap()
            })
            .await
            .expect("the sibling must still serve RPC after the refusal");
            assert_eq!(response, b"sibling-rpc");
            reply.await.unwrap();

            for task in spawned_tasks.close() {
                let _ = task.await;
            }
            local.close().await;
            peer.close().await;
            drop(refused_outbound);
        })
        .unwrap();
}
