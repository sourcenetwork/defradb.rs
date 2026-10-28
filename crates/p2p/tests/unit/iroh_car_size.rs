use super::super::endpoint_config::AllowlistState;
use super::*;
use crate::iroh::fetch_stats::FetchStats;
use crate::sync::car::{decode_car_oversized, encode_car_response, CAR_MAX_BYTES};
use multihash_codetable::{Code, MultihashDigest};
use nuclei::sync::oneshot;

async fn serve(endpoint: Endpoint, response: Vec<u8>, ready: Option<oneshot::Receiver<()>>) {
    let connection = endpoint.accept().await.unwrap().await.unwrap();
    let (mut send, mut recv) = connection.accept_bi().await.unwrap();
    assert_eq!(
        protocols::read_stream_tag(&mut recv).await.unwrap(),
        protocols::STREAM_CAR
    );
    recv.read_to_end(4096).await.unwrap();
    if let Some(ready) = ready {
        ready.await.unwrap();
    }
    send.write_all(&response).await.unwrap();
    send.finish().unwrap();
    let _ = connection.closed().await;
}

#[nuclei::test]
async fn size_notice_does_not_cancel_alternate_provider_or_emit_generic_failure() {
    edge::compat(async {
        nuclei::time::timeout(std::time::Duration::from_secs(10), async {
            for alternate in [false, true] {
                let client = tests::localhost_endpoint(vec![]).await;
                let limited = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
                let healthy = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
                let root = cid::Cid::new_v1(0x71, Code::Sha2_256.digest(b"block"));
                let notice =
                    encode_car_response(&[root], &[], &[(root, CAR_MAX_BYTES + 1)]).unwrap();
                let limited_task =
                    nuclei::spawn(edge::compat(serve(limited.clone(), notice, None)));
                let (ready_tx, ready_rx) = oneshot::channel();
                let healthy_task = alternate.then(|| {
                    nuclei::spawn(edge::compat(serve(
                        healthy.clone(),
                        encode_car_response(&[root], &[(&root, b"block")], &[]).unwrap(),
                        Some(ready_rx),
                    )))
                });
                let cache = new_connection_cache(GossipHandle::new());
                let admission = Arc::new(PeerAdmission::new(AllowlistState::AcceptAll));
                let peer_map = Arc::new(SharedPeerMap::new());
                let endpoints = if alternate {
                    vec![&limited, &healthy]
                } else {
                    vec![&limited]
                };
                let mut providers = Vec::new();
                for endpoint in endpoints {
                    let provider = PeerId::new(endpoint.id().to_string());
                    let addr = endpoint.addr().ip_addrs().next().copied().unwrap();
                    let connection =
                        connect_with_cache(&client, &provider, Some(addr), &cache, &admission)
                            .await
                            .unwrap();
                    peer_map.increment_connections(endpoint.id(), Some(addr), connection);
                    providers.push(provider);
                }
                let (tx, mut rx) = crate::sync::dispatch_queue(8);
                let task = nuclei::spawn(edge::compat(handle_block_sync(
                    BlockSyncResources::new(
                        client.clone(),
                        peer_map,
                        cache,
                        tx,
                        admission,
                        Default::default(),
                    ),
                    QueryId(88),
                    root,
                    providers,
                    vec![root],
                )));
                let Some(TransportEvent::CarFetchResponse {
                    query_id, car_data, ..
                }) = rx.recv().await
                else {
                    panic!("expected size-limit CAR response");
                };
                assert_eq!(query_id, Some(QueryId(88)));
                assert_eq!(
                    decode_car_oversized(&car_data).unwrap(),
                    vec![(root, CAR_MAX_BYTES + 1)]
                );
                if alternate {
                    ready_tx.send(()).unwrap();
                    let Some(TransportEvent::CarFetchResponse { car_data, .. }) = rx.recv().await
                    else {
                        panic!("size-limited response must not cancel the healthy provider");
                    };
                    assert!(crate::sync::car::car_has_any_block(&car_data));
                    assert!(decode_car_oversized(&car_data).unwrap().is_empty());
                }
                task.join().await.unwrap();
                assert!(
                    rx.try_recv().is_none(),
                    "no generic failure may overwrite the size notice"
                );
                client.close().await;
                limited.close().await;
                healthy.close().await;
                limited_task.abort();
                if let Some(task) = healthy_task {
                    task.abort();
                }
            }
        })
        .await
        .expect("CAR providers should finish promptly");
    })
    .await
}

#[nuclei::test]
async fn each_provider_fetch_is_measured() {
    edge::compat(async {
        nuclei::time::timeout(std::time::Duration::from_secs(10), async {
            let client = tests::localhost_endpoint(vec![]).await;
            let limited = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let healthy = tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let root = cid::Cid::new_v1(0x71, Code::Sha2_256.digest(b"block"));
            let notice = encode_car_response(&[root], &[], &[(root, CAR_MAX_BYTES + 1)]).unwrap();
            let blocks = encode_car_response(&[root], &[(&root, b"block")], &[]).unwrap();
            let blocks_len = blocks.len() as u64;
            let (ready_tx, ready_rx) = oneshot::channel();
            let limited_task = nuclei::spawn(edge::compat(serve(limited.clone(), notice, None)));
            let healthy_task =
                nuclei::spawn(edge::compat(serve(healthy.clone(), blocks, Some(ready_rx))));

            let cache = new_connection_cache(GossipHandle::new());
            let admission = Arc::new(PeerAdmission::new(AllowlistState::AcceptAll));
            let peer_map = Arc::new(SharedPeerMap::new());
            let mut providers = Vec::new();
            for endpoint in [&limited, &healthy] {
                let provider = PeerId::new(endpoint.id().to_string());
                let addr = endpoint.addr().ip_addrs().next().copied().unwrap();
                let connection =
                    connect_with_cache(&client, &provider, Some(addr), &cache, &admission)
                        .await
                        .unwrap();
                peer_map.increment_connections(endpoint.id(), Some(addr), connection);
                providers.push(provider);
            }
            let stats = Arc::new(FetchStats::default());
            let (tx, mut rx) = crate::sync::dispatch_queue(8);
            let task = nuclei::spawn(edge::compat(handle_block_sync(
                BlockSyncResources::new(
                    client.clone(),
                    peer_map,
                    cache,
                    tx,
                    admission,
                    Arc::clone(&stats),
                ),
                QueryId(89),
                root,
                providers.clone(),
                vec![root],
            )));
            rx.recv().await.expect("size notice");
            ready_tx.send(()).unwrap();
            rx.recv().await.expect("blocks");
            task.join().await.unwrap();

            let of = |provider: &PeerId| {
                stats
                    .snapshot()
                    .into_iter()
                    .find(|peer| peer.peer == provider.to_string())
                    .expect("provider measured")
            };
            let limited_stats = of(&providers[0]);
            assert_eq!(limited_stats.fetches, 1);
            assert_eq!(limited_stats.partial_ppm, 1_000_000);
            let healthy_stats = of(&providers[1]);
            assert_eq!(healthy_stats.fetches, 1);
            assert_eq!(healthy_stats.success_ppm, 1_000_000);
            assert_eq!(healthy_stats.bytes, blocks_len);
            assert!(healthy_stats.min_rtt_micros.is_some());
            assert_eq!(
                (healthy_stats.in_flight, healthy_stats.in_flight_bytes),
                (0, 0)
            );

            client.close().await;
            limited.close().await;
            healthy.close().await;
            limited_task.abort();
            healthy_task.abort();
        })
        .await
        .expect("CAR providers should finish promptly");
    })
    .await
}
