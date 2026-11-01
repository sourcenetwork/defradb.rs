use super::*;

use crate::message::{IdentityRequest, IdentityResponse, IDENTITY_UNCONFIGURED_ERROR};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::endpoint::EndpointResources;
use super::super::endpoint_config::AllowlistState;
use super::super::endpoint_streams::handle_incoming;
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
                let cache = new_connection_cache(GossipHandle::new());
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
            let cache = new_connection_cache(GossipHandle::new());
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
    let resources = EndpointResources {
        endpoint: endpoint.clone(),
        gossip: gossip.clone(),
        peer_map: Arc::new(SharedPeerMap::new()),
        connection_cache: new_connection_cache(gossip),
        spawned_tasks: Arc::new(TaskRegistry::default()),
        node_identity: None,
        admission: Arc::new(PeerAdmission::new(AllowlistState::AcceptAll)),
        inbound_budget: Arc::default(),
        fetch_stats: Arc::default(),
        records: RecordsHandle::new(),
        solicited: Arc::new(Solicited::new()),
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
    let (events, mut received) = crate::sync::dispatch_queue(8);
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
    EndpointResources {
        endpoint,
        gossip: gossip.clone(),
        peer_map: Arc::new(SharedPeerMap::new()),
        connection_cache: new_connection_cache(gossip),
        spawned_tasks: Arc::new(TaskRegistry::default()),
        node_identity: None,
        admission: Arc::new(PeerAdmission::new(AllowlistState::AcceptAll)),
        inbound_budget: Arc::default(),
        fetch_stats: Arc::default(),
        records: RecordsHandle::new(),
        solicited: Arc::new(Solicited::new()),
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
            let cache = new_connection_cache(GossipHandle::new());
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
