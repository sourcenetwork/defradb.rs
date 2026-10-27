use super::*;

#[tokio::test]
async fn cancelling_the_producer_wakes_joiners_and_preserves_progress() {
    let store = CountingReads::new(0);
    let (leaf, data) = ipld_block(1);
    store.put(&leaf, &data).await.unwrap();
    let data = DagCborCodec::encode_to_vec(&ipld!({"child": leaf})).unwrap();
    let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
    store.put(&root, &data).await.unwrap();
    let progress = limits::progress_with_budget(3);
    let requested = RapidHashSet::from_iter([leaf]);
    let mut producer = Box::pin(progress.walk(&store, root, requested.clone()));
    assert!(futures::poll!(producer.as_mut()).is_pending());
    let mut joined = Box::pin(progress.walk(&store, root, requested.clone()));
    assert!(futures::poll!(joined.as_mut()).is_pending());
    drop(producer);
    assert!(matches!(
        tokio::time::timeout(Duration::from_millis(100), joined)
            .await
            .unwrap(),
        Err(Error::ResponseTimeout)
    ));
    assert_eq!(progress.cid_budget.used(), 2);
    store.block_read.add_permits(2);
    assert_eq!(
        progress
            .walk(&store, root, requested.clone())
            .await
            .unwrap(),
        requested
    );
    assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert_eq!(progress.cid_budget.used(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_joiners_deadline_does_not_cancel_the_producer() {
    let store = CountingReads::new(0);
    let (root, data) = ipld_block(1);
    store.put(&root, &data).await.unwrap();
    let progress = RootedAuthorizationProgress::new();
    let requested = RapidHashSet::from_iter([root]);
    let mut producer = Box::pin(progress.walk(&store, root, requested.clone()));
    assert!(futures::poll!(producer.as_mut()).is_pending());
    assert!(matches!(
        progress
            .walk_with_limits(
                &store,
                root,
                requested.clone(),
                Duration::from_millis(1),
                100
            )
            .await,
        Err(Error::ResponseTimeout)
    ));
    store.block_read.add_permits(1);
    assert_eq!(producer.await.unwrap(), requested);
    assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(progress.cid_budget.used(), 0);
}
