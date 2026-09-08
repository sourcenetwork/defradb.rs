use super::*;

#[tokio::test(start_paused = true)]
async fn unservable_batch_does_not_hide_later_servable_blocks() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let blocks: HashMap<_, _> = (0..2050)
        .map(|i| {
            let data = encode_ipld(ipld!({ "value": i }));
            (make_cid(&data), data)
        })
        .collect();
    let links: Vec<_> = blocks.keys().copied().map(Ipld::Link).collect();
    let root_data = encode_ipld(ipld!({ "children": links }));
    let root = make_cid(&root_data);
    blockstore.put(&root, &root_data).await.unwrap();
    let missing =
        crate::sync::manager::links::find_all_missing_links(blockstore.as_ref(), &root_data)
            .await
            .unwrap();
    let oversized = missing[0];
    let last = *missing.last().unwrap();
    let transport = TestTransport::new(
        blockstore.clone(),
        root,
        root_data,
        HashMap::new(),
        HashMap::from([(last, blocks[&last].clone())]),
    );
    let completions = crate::sync::manager::BlockSyncCompletionTracker::default();
    transport
        .size_limited_providers
        .lock()
        .unwrap()
        .insert("remote-peer".into(), (oversized, completions.clone()));
    let context = DagFetchContext::new(
        "doc".into(),
        "collection".into(),
        String::new(),
        PeerId::new("remote-peer".into()),
    )
    .with_block_sync_completions(completions);
    let (tx, mut rx) = mpsc::channel(4);
    poll_fetch_dag(
        transport,
        blockstore.clone(),
        tx,
        root,
        context,
        DagFetchLimiter::new(1),
        diagnostics(),
    )
    .await;
    assert!(
        blockstore.has(&last).await.unwrap(),
        "later batch was skipped"
    );
    assert!(!blockstore.has(&oversized).await.unwrap());
    assert!(rx.try_recv().is_err(), "incomplete DAG reported ready");
}

#[tokio::test(start_paused = true)]
async fn size_limited_provider_is_not_retried_but_alternate_can_finish() {
    for alternate in [false, true] {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let child_data = encode_ipld(ipld!({ "value": "child" }));
        let child = make_cid(&child_data);
        let root_data = encode_ipld(ipld!({ "child": child }));
        let root = make_cid(&root_data);
        blockstore.put(&root, &root_data).await.unwrap();
        let transport = TestTransport::new(
            blockstore.clone(),
            root,
            root_data,
            HashMap::new(),
            HashMap::from([(child, child_data)]),
        );
        let completions = crate::sync::manager::BlockSyncCompletionTracker::default();
        transport
            .size_limited_providers
            .lock()
            .unwrap()
            .insert("remote-peer".to_owned(), (child, completions.clone()));
        let mut context = DagFetchContext::new(
            "doc".to_owned(),
            "collection".to_owned(),
            String::new(),
            PeerId::new("remote-peer".to_owned()),
        )
        .with_block_sync_completions(completions);
        if alternate {
            context = context.with_alternate_providers(vec![PeerId::new("alt-peer".to_owned())]);
        }
        let (tx, mut rx) = mpsc::channel(4);
        let start = tokio::time::Instant::now();
        poll_fetch_dag(
            transport.clone(),
            blockstore.clone(),
            tx,
            root,
            context,
            DagFetchLimiter::new(1),
            diagnostics(),
        )
        .await;
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "must not wait for another retry attempt"
        );
        assert_eq!(
            transport.sync_providers(),
            if alternate {
                vec!["remote-peer", "alt-peer"]
            } else {
                vec!["remote-peer"]
            }
        );
        assert_eq!(
            transport.cancelled_queries().len(),
            if alternate { 2 } else { 1 }
        );
        assert_eq!(blockstore.has(&child).await.unwrap(), alternate);
        assert_eq!(rx.try_recv().is_ok(), alternate);
    }
}
