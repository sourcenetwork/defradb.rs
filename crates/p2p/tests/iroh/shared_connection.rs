use super::*;

use crate::message::{IdentityRequest, IdentityResponse, IDENTITY_UNCONFIGURED_ERROR};

use super::super::endpoint::EndpointResources;
use super::super::endpoint_config::AllowlistState;
use super::super::endpoint_streams::handle_incoming;
use super::super::records::RecordsHandle;
use super::super::solicited::Solicited;
use super::super::task_registry::TaskRegistry;

#[test]
fn disconnect_closes_replacement_despite_stale_cleanup() {
    edge::testing::edge()
        .block_on(edge::compat(async {
            for alpn in [protocols::ALPN_MUX, protocols::ALPN_MUX_V1] {
                let endpoint = tests::localhost_endpoint(vec![]).await;
                let peer = tests::localhost_endpoint(vec![alpn.to_vec()]).await;
                let (old, accepted_old) =
                    futures::join!(endpoint.connect(peer.addr(), alpn), async {
                        peer.accept().await.unwrap().await.unwrap()
                    },);
                let old = old.unwrap();
                let (replacement, accepted_replacement) =
                    futures::join!(endpoint.connect(peer.addr(), alpn), async {
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
        }))
        .unwrap();
}

#[test]
fn legacy_only_peer_reuses_and_refreshes_its_shared_rpc_path() {
    edge::testing::edge()
        .block_on(edge::compat(async {
            let endpoint = tests::localhost_endpoint(vec![]).await;
            let peer = tests::localhost_endpoint(vec![protocols::ALPN_MUX_V1.to_vec()]).await;
            let id = PeerId::new(peer.id().to_string());
            let direct = peer.addr().ip_addrs().next().copied();
            let cache = new_connection_cache(GossipHandle::new());
            let admission = PeerAdmission::new(AllowlistState::AcceptAll);
            let (first, accepted_first) = futures::join!(
                connect_with_cache(&endpoint, &id, direct, &cache, &admission),
                async { peer.accept().await.unwrap().await.unwrap() },
            );
            let first = first.unwrap();
            assert_eq!(first.alpn(), protocols::ALPN_MUX_V1);
            let reused = crate::rt::time::timeout(
                std::time::Duration::from_secs(2),
                connect_with_cache(&endpoint, &id, direct, &cache, &admission),
            )
            .await
            .expect("a cache hit must not require another accept")
            .unwrap();
            assert_eq!(first.stable_id(), reused.stable_id());

            let (replacement, accepted_replacement) = futures::join!(
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
        }))
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
        spawned_tasks: Arc::new(TaskRegistry::new(edge::testing::edge())),
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
        let (outbound, inbound) = futures::join!(
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
    let (incoming, ()) = futures::join!(
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
    let reply = crate::rt::spawn(edge::compat(async move {
        let (mut send, mut recv) = incoming.accept_bi().await.unwrap();
        let request = recv.read_to_end(32).await.unwrap();
        send.write_all(&request).await.unwrap();
        send.finish().unwrap();
        // The peer's last connection handle must outlive acknowledgement of its reply.
        assert!(send.stopped().await.unwrap().is_none());
    }));
    let response = crate::rt::time::timeout(std::time::Duration::from_secs(2), async {
        let (mut send, mut recv) = shared.open_bi().await.unwrap();
        send.write_all(b"reverse-rpc").await.unwrap();
        send.finish().unwrap();
        recv.read_to_end(32).await.unwrap()
    })
    .await
    .expect("reverse RPC must reach the accepted connection");
    assert_eq!(response, b"reverse-rpc");
    reply.join().await.unwrap();
    if !retain_old {
        crate::rt::time::timeout(std::time::Duration::from_secs(2), async {
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
        .block_on(edge::compat(async {
            reverse_rpc_uses_inbound(false).await;
        }))
        .unwrap();
}

#[test]
fn authenticated_reconnection_replaces_an_open_old_rpc_path() {
    edge::testing::edge()
        .block_on(edge::compat(async {
            reverse_rpc_uses_inbound(true).await;
        }))
        .unwrap();
}

#[test]
fn legacy_mux_does_not_replace_the_modern_shared_rpc_handle() {
    edge::testing::edge()
        .block_on(edge::compat(async {
            let endpoint = tests::localhost_endpoint(vec![]).await;
            let peer = tests::localhost_endpoint(vec![
                protocols::ALPN_MUX.to_vec(),
                protocols::ALPN_MUX_V1.to_vec(),
            ])
            .await;
            let (modern, accepted_modern) =
                futures::join!(endpoint.connect(peer.addr(), protocols::ALPN_MUX), async {
                    peer.accept().await.unwrap().await.unwrap()
                },);
            let modern = modern.unwrap();
            let cache = new_connection_cache(GossipHandle::new());
            remember_connection(&cache, &modern);
            let (legacy, accepted_legacy) = futures::join!(
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
        }))
        .unwrap();
}

fn managed_resources(endpoint: Endpoint) -> EndpointResources {
    let gossip = GossipHandle::new();
    EndpointResources {
        endpoint,
        gossip: gossip.clone(),
        peer_map: Arc::new(SharedPeerMap::new()),
        connection_cache: new_connection_cache(gossip),
        spawned_tasks: Arc::new(TaskRegistry::new(edge::testing::edge())),
        node_identity: None,
        admission: Arc::new(PeerAdmission::new(AllowlistState::AcceptAll)),
        inbound_budget: Arc::default(),
        fetch_stats: Arc::default(),
        records: RecordsHandle::new(),
        solicited: Arc::new(Solicited::new()),
    }
}

async fn identity_rpc(
    from: &EndpointResources,
    to: &Endpoint,
    direct: Option<std::net::SocketAddr>,
    direction: &str,
) -> IdentityResponse {
    let peer = PeerId::new(to.id().to_string());
    crate::rt::time::timeout(
        std::time::Duration::from_secs(2),
        handle_request_response::<_, IdentityResponse>(
            &from.endpoint,
            &peer,
            protocols::STREAM_IDENTITY,
            &IdentityRequest::new(from.endpoint.id().to_string()),
            direct,
            &from.connection_cache,
            &from.admission,
        ),
    )
    .await
    .unwrap_or_else(|_| panic!("the {direction} identity request must be served"))
    .unwrap()
}

/// The dialer's only path to the acceptor is the RPC cache miss, so the
/// acceptor's reverse request can only be served by the dialer pumping the
/// connection that cache miss created.
#[test]
fn rpc_dialed_connection_serves_reverse_service_request() {
    edge::testing::edge()
        .block_on(edge::compat(async {
            let dialer = managed_resources(tests::localhost_endpoint(vec![]).await);
            let acceptor = managed_resources(
                tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await,
            );
            let (events, _received) = crate::sync::dispatch_queue(8);
            let accept_loop = crate::rt::spawn(edge::compat({
                let acceptor = acceptor.clone();
                async move {
                    let incoming = acceptor.endpoint.accept().await.unwrap();
                    handle_incoming(incoming, &acceptor, &events).await;
                    futures::future::pending::<()>().await;
                }
            }));

            let direct = acceptor.endpoint.addr().ip_addrs().next().copied();
            let forward = identity_rpc(&dialer, &acceptor.endpoint, direct, "forward").await;
            assert_eq!(
                forward.err_message.as_deref(),
                Some(IDENTITY_UNCONFIGURED_ERROR)
            );
            let dialed = cached_connection(
                &dialer.connection_cache,
                &PeerId::new(acceptor.endpoint.id().to_string()),
            )
            .unwrap()
            .expect("the RPC dial must cache the connection it created");

            let reverse = identity_rpc(&acceptor, &dialer.endpoint, None, "reverse").await;
            assert_eq!(
                reverse.err_message.as_deref(),
                Some(IDENTITY_UNCONFIGURED_ERROR)
            );
            assert_eq!(
                cached_connection(
                    &dialer.connection_cache,
                    &PeerId::new(acceptor.endpoint.id().to_string()),
                )
                .unwrap()
                .unwrap()
                .stable_id(),
                dialed.stable_id(),
                "the reverse request must ride the dialed connection"
            );

            accept_loop.abort();
            for resources in [&dialer, &acceptor] {
                for task in resources.spawned_tasks.close() {
                    let _ = task.await;
                }
                resources.endpoint.close().await;
            }
        }))
        .unwrap();
}
