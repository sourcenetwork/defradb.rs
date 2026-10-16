use super::*;

pub(super) fn progress_with_budget(limit: usize) -> RootedAuthorizationProgress {
    RootedAuthorizationProgress {
        cid_budget: CidBudget::new(limit),
        ..RootedAuthorizationProgress::default()
    }
}

#[test]
fn concurrent_reservations_cannot_oversubscribe_the_shared_budget() {
    let budget = CidBudget::new(8);
    let barrier = Arc::new(std::sync::Barrier::new(17));
    let workers: Vec<_> = (0..16)
        .map(|_| {
            let budget = budget.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let reservation = budget.reserve(1);
                barrier.wait();
                barrier.wait();
                reservation.is_some()
            })
        })
        .collect();
    barrier.wait();
    let used = budget.used();
    barrier.wait();
    let admitted = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .filter(|admitted| *admitted)
        .count();
    assert_eq!(used, 8);
    assert_eq!(admitted, 8);
    assert_eq!(budget.used(), 0);
}

#[tokio::test]
async fn final_requested_read_error_preserves_the_reached_grant_for_retry() {
    let store = CountingReads::new(10);
    let (root, data) = ipld_block(1);
    store.put(&root, &data).await.unwrap();
    store
        .fail_read_once
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let progress = progress_with_budget(2);
    let requested = RapidHashSet::from_iter([root]);
    assert!(progress
        .walk(&store, root, requested.clone())
        .await
        .is_err());
    assert_eq!(progress.cid_budget.used(), 2);
    assert_eq!(
        progress
            .walk(&store, root, requested.clone())
            .await
            .unwrap(),
        requested
    );
    assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(progress.cid_budget.used(), 0);
}

#[tokio::test]
async fn missing_branch_preserves_exact_reached_grants_across_retries() {
    let store = CountingReads::new(100);
    let (leaf, data) = ipld_block(1000);
    store.put(&leaf, &data).await.unwrap();
    let (unrelated, data) = ipld_block(1001);
    store.put(&unrelated, &data).await.unwrap();
    let missing_data = DagCborCodec::encode_to_vec(&ipld!({"child": unrelated})).unwrap();
    let missing = Cid::new_v1(0x71, Code::Sha2_256.digest(&missing_data));
    let data = DagCborCodec::encode_to_vec(&ipld!({"a": missing, "b": leaf})).unwrap();
    let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
    store.put(&root, &data).await.unwrap();
    let progress = RootedAuthorizationProgress::new();
    let requested = RapidHashSet::from_iter([leaf, unrelated, missing]);
    for _ in 0..5 {
        assert_eq!(
            progress
                .walk(&store, root, requested.clone())
                .await
                .unwrap(),
            RapidHashSet::from_iter([leaf])
        );
    }
    assert_eq!(store.reads.load(std::sync::atomic::Ordering::SeqCst), 2);
    store.put(&missing, &missing_data).await.unwrap();
    assert_eq!(
        progress
            .walk(&store, root, requested.clone())
            .await
            .unwrap(),
        requested
    );
    assert!(progress.cursors.lock().unwrap().entries.is_empty());
    assert_eq!(progress.cid_budget.used(), 0);
}

#[tokio::test]
async fn wide_frontier_exhausts_retention_before_growing_past_the_cap() {
    let store = CountingReads::new(100);
    let links: Vec<_> = (0..10)
        .map(|value| Ipld::Link(ipld_block(value).0))
        .collect();
    let data = DagCborCodec::encode_to_vec(&ipld!({"children": links})).unwrap();
    let root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
    store.put(&root, &data).await.unwrap();
    let (unrelated, _) = ipld_block(1000);
    let progress = progress_with_budget(4);
    assert!(matches!(
        progress
            .walk(&store, root, RapidHashSet::from_iter([root, unrelated]))
            .await,
        Err(Error::ResponseTimeout)
    ));
    assert!(progress.cursors.lock().unwrap().entries.is_empty());
    assert_eq!(progress.cid_budget.used(), 0);
    // A root-only request must not spend the budget on unneeded children.
    assert_eq!(
        progress
            .walk(&store, root, RapidHashSet::from_iter([root]))
            .await
            .unwrap(),
        RapidHashSet::from_iter([root])
    );
    assert_eq!(progress.cid_budget.used(), 0);
}

#[tokio::test]
async fn distinct_want_lists_share_one_retention_budget() {
    let store = CountingReads::new(10_000);
    let (mut root, _) = ipld_block(777);
    for value in 0..20 {
        let data = DagCborCodec::encode_to_vec(&ipld!({"child": root, "value": value})).unwrap();
        root = Cid::new_v1(0x71, Code::Sha2_256.digest(&data));
        store.put(&root, &data).await.unwrap();
    }
    let progress = progress_with_budget(48);
    let mut exhausted = 0;
    for value in 1000..1070 {
        let requested = RapidHashSet::from_iter([ipld_block(value).0]);
        for _ in 0..3 {
            match progress
                .walk_with_limits(&store, root, requested.clone(), Duration::from_secs(30), 10)
                .await
            {
                Ok(granted) => assert!(granted.is_empty()),
                Err(error) => {
                    assert!(matches!(error, Error::ResponseTimeout));
                    exhausted += 1;
                }
            }
            assert!(progress.cid_budget.used() <= 48);
            assert!(progress.cursors.lock().unwrap().entries.len() <= 2);
        }
    }
    assert!(exhausted > 0);
    let cursors: Vec<_> = progress
        .cursors
        .lock()
        .unwrap()
        .entries
        .values()
        .map(|entry| entry.cursor.clone())
        .collect();
    let mut retained = 0;
    for cursor in cursors {
        let state = cursor.lock().await;
        retained += state.remaining.len() + state.authorized.len() + state.seen.len();
    }
    assert_eq!(progress.cid_budget.used(), retained);
}

#[tokio::test]
async fn oversized_want_list_does_not_evict_an_existing_cursor() {
    let store = CountingReads::new(10);
    let (root, _) = ipld_block(0);
    let progress = progress_with_budget(3);
    assert!(progress
        .walk(&store, root, RapidHashSet::from_iter([root]))
        .await
        .unwrap()
        .is_empty());
    let requested = (1..=3).map(|value| ipld_block(value).0).collect();
    assert!(progress.walk(&store, root, requested).await.is_err());
    assert_eq!(progress.cursors.lock().unwrap().entries.len(), 1);
    assert_eq!(progress.cid_budget.used(), 2);
}

#[tokio::test]
async fn spare_want_list_capacity_is_not_retained() {
    let store = CountingReads::new(10);
    let (root, _) = ipld_block(1);
    let mut requested = RapidHashSet::with_capacity(1024);
    requested.insert(root);
    let progress = progress_with_budget(2);
    assert!(progress
        .walk(&store, root, requested)
        .await
        .unwrap()
        .is_empty());
    let cursor = progress
        .cursors
        .lock()
        .unwrap()
        .entries
        .values()
        .next()
        .unwrap()
        .cursor
        .clone();
    assert!(cursor.lock().await.remaining.capacity() <= 3);
    assert_eq!(progress.cid_budget.used(), 2);
}

#[tokio::test]
async fn idle_eviction_releases_its_retention_charge() {
    let store = CountingReads::new(10);
    let progress = progress_with_budget(2);
    let (first, _) = ipld_block(1);
    let (second, _) = ipld_block(2);
    for root in [first, second] {
        assert!(progress
            .walk(&store, root, RapidHashSet::from_iter([root]))
            .await
            .unwrap()
            .is_empty());
        assert_eq!(progress.cid_budget.used(), 2);
    }
    let cursors = progress.cursors.lock().unwrap();
    assert_eq!(cursors.entries.len(), 1);
    assert_eq!(cursors.entries.keys().next().unwrap().root, second);
}

#[tokio::test(start_paused = true)]
async fn expired_idle_cursors_release_the_budget_on_next_request() {
    let store = CountingReads::new(10);
    let progress = progress_with_budget(8);
    let (first, _) = ipld_block(1);
    let (second, _) = ipld_block(2);
    assert!(progress
        .walk(&store, first, RapidHashSet::from_iter([first]))
        .await
        .unwrap()
        .is_empty());
    tokio::time::advance(CURSOR_IDLE_TTL).await;
    assert!(progress
        .walk(&store, second, RapidHashSet::from_iter([second]))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(progress.cid_budget.used(), 2);
    let cursors = progress.cursors.lock().unwrap();
    assert_eq!(cursors.entries.len(), 1);
    assert_eq!(cursors.entries.keys().next().unwrap().root, second);
}

#[tokio::test(start_paused = true)]
async fn idle_expiry_does_not_remove_an_active_cursor() {
    let store = CountingReads::new(0);
    let (root, data) = ipld_block(1);
    store.put(&root, &data).await.unwrap();
    let (wanted, _) = ipld_block(2);
    let (missing, _) = ipld_block(3);
    let progress = progress_with_budget(4);
    let mut active = Box::pin(progress.walk(&store, root, RapidHashSet::from_iter([wanted])));
    assert!(futures::poll!(active.as_mut()).is_pending());
    tokio::time::advance(CURSOR_IDLE_TTL).await;
    assert!(progress
        .walk(&store, missing, RapidHashSet::from_iter([missing]))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(progress.cursors.lock().unwrap().entries.len(), 2);
    assert_eq!(progress.cid_budget.used(), 4);
    drop(active);
    assert!(progress
        .walk(&store, missing, RapidHashSet::from_iter([missing]))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(progress.cursors.lock().unwrap().entries.len(), 1);
    assert_eq!(progress.cid_budget.used(), 2);
}
