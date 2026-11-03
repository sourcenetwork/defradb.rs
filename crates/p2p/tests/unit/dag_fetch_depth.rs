use super::*;

/// The provider holds a 100-block chain and the receiver its oldest `held`
/// blocks. Each round serves only its cut below the frontier: a new head over
/// held history costs one small round, and an empty receiver widens the cut
/// without any round re-serving an earlier round's blocks.
#[nuclei::test(threads = 0)]
async fn depth_cut_fetch_serves_only_the_cut_below_the_frontier() {
    let cases = [
        (99, vec![(2, 3)]),
        (
            0,
            vec![(2, 3), (4, 5), (8, 9), (16, 17), (32, 33), (64, 33)],
        ),
    ];
    let store = || {
        Arc::new(DefraBlockstore::new(
            Arc::new(RegolithStore::in_memory().unwrap()),
            true,
        ))
    };
    for (held, rounds) in cases {
        let (provider, receiver) = (store(), store());
        let mut blocks: Vec<(Cid, Vec<u8>)> = Vec::new();
        for idx in 0..100i64 {
            let data = match blocks.last() {
                Some((prev, _)) => encode_ipld(ipld!({ "idx": idx, "prev": *prev })),
                None => encode_ipld(ipld!({ "idx": idx })),
            };
            let cid = make_cid(&data);
            provider.put(&cid, &data).await.unwrap();
            if blocks.len() < held {
                receiver.put(&cid, &data).await.unwrap();
            }
            blocks.push((cid, data));
        }
        let (head, head_data) = blocks.last().unwrap().clone();
        let transport = TestTransport::new(
            receiver.clone(),
            head,
            head_data,
            RapidHashMap::new(),
            RapidHashMap::new(),
        );
        transport.set_force_rooted_sync();
        transport.set_closure_provider(provider);
        let (event_tx, mut event_rx) = mpsc::channel(1);

        poll_fetch_dag(
            transport.clone(),
            receiver.clone(),
            event_tx,
            head,
            DagFetchContext::new(
                "doc-id".to_string(),
                "collection-id".to_string(),
                "creator-id".to_string(),
                PeerId::new("remote-peer".to_string()),
            ),
            DagFetchLimiter::new(2),
            diagnostics(),
        )
        .await;

        assert!(matches!(
            event_rx.next().await,
            Some(SyncEvent::DagReady { root_cid, .. }) if root_cid == head
        ));
        for (cid, _) in &blocks {
            assert!(receiver.has(cid).await.unwrap());
        }
        assert_eq!(transport.car_request_count(), 0);
        assert_eq!(transport.closure_rounds(), rounds, "held {held}");
        if held == 99 {
            assert_eq!(transport.sync_batches(), vec![vec![head]]);
        }
    }
}
