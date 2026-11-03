use super::super::endpoint_config::AllowlistState;
use super::*;
use crate::sync::car::encode_car_response;
use multihash_codetable::{Code, MultihashDigest};

/// Answer one CAR request with `root`, returning the request once the client closes.
async fn serve_capturing(endpoint: Endpoint, root: cid::Cid) -> CarFetchRequest {
    let connection = endpoint.accept().await.unwrap().await.unwrap();
    let (mut send, mut recv) = connection.accept_bi().await.unwrap();
    assert_eq!(
        protocols::read_stream_tag(&mut recv).await.unwrap(),
        protocols::STREAM_CAR
    );
    let framed = recv.read_to_end(4096).await.unwrap();
    let request: CarFetchRequest = protocols::decode(&framed[4..]).unwrap();
    let response = encode_car_response(&[root], &[(&root, b"block")], &[]).unwrap();
    send.write_all(&response).await.unwrap();
    send.finish().unwrap();
    let _ = connection.closed().await;
    request
}

/// The receiver's depth cut reaches the provider on the wire, and a sync
/// without one still asks for the whole closure.
#[tokio::test]
async fn block_sync_sends_the_receivers_depth_cut() {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        for max_depth in [Some(3), None] {
            let client = tests::localhost_endpoint(vec![]).await;
            let provider_endpoint =
                tests::localhost_endpoint(vec![protocols::ALPN_MUX.to_vec()]).await;
            let root = cid::Cid::new_v1(0x71, Code::Sha2_256.digest(b"block"));
            let serve_task = tokio::spawn(serve_capturing(provider_endpoint.clone(), root));
            let cache = new_unmanaged_connection_cache(GossipHandle::new());
            let admission = Arc::new(PeerAdmission::new(AllowlistState::AcceptAll));
            let peer_map = Arc::new(SharedPeerMap::new());
            let provider = PeerId::new(provider_endpoint.id().to_string());
            let addr = provider_endpoint.addr().ip_addrs().next().copied().unwrap();
            let connection = connect_with_cache(&client, &provider, Some(addr), &cache, &admission)
                .await
                .unwrap();
            peer_map.increment_connections(provider_endpoint.id(), Some(addr), connection);
            let (tx, mut rx) = crate::sync::dispatch_queue(8);

            handle_block_sync(
                BlockSyncResources::new(
                    client.clone(),
                    peer_map,
                    cache,
                    tx,
                    admission,
                    Default::default(),
                ),
                QueryId(90),
                root,
                vec![provider],
                vec![root],
                max_depth,
            )
            .await;

            assert!(matches!(
                rx.recv().await,
                Some(TransportEvent::CarFetchResponse { .. })
            ));
            client.close().await;
            let expected = CarFetchRequest {
                max_depth,
                ..CarFetchRequest::selective_dag(root, vec![root])
            };
            assert_eq!(serve_task.await.unwrap(), expected);
            provider_endpoint.close().await;
        }
    })
    .await
    .expect("CAR provider should answer promptly");
}
