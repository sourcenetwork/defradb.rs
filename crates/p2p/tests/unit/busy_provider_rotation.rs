use super::*;
use crate::sync::manager::{BlockSyncCompletionTracker, RootedCarCompletionTracker};

struct Fixture {
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    root: Cid,
    child: Cid,
    completions: BlockSyncCompletionTracker,
    rooted_car: RootedCarCompletionTracker,
    transport: TestTransport,
    diagnostics: Arc<SyncDiagnostics>,
}

impl Fixture {
    async fn new(busy: &[&str], root_stored: bool) -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let (root, root_data, child, child_data) = single_child_dag();
        if root_stored {
            blockstore.put(&root, &root_data).await.unwrap();
        }
        let transport = TestTransport::new(
            blockstore.clone(),
            root,
            root_data.clone(),
            RapidHashMap::new(),
            RapidHashMap::from_iter([(root, root_data), (child, child_data)]),
        );
        let completions = BlockSyncCompletionTracker::default();
        for peer in busy {
            transport
                .busy_providers
                .insert((*peer).to_string(), completions.clone());
        }
        Self {
            blockstore,
            root,
            child,
            completions,
            rooted_car: RootedCarCompletionTracker::default(),
            transport,
            diagnostics: diagnostics(),
        }
    }

    fn lone_context(&self) -> DagFetchContext {
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("remote-peer".to_string()),
        )
        .with_block_sync_completions(self.completions.clone())
    }

    fn rooted_context(&self) -> DagFetchContext {
        self.lone_context()
            .with_rooted_car_completions(self.rooted_car.clone())
    }

    fn alt_context(&self) -> DagFetchContext {
        self.lone_context()
            .with_alternate_providers(vec![PeerId::new("alt-peer".to_string())])
    }

    async fn fetch_dag(&self, context: DagFetchContext) -> Option<SyncEvent> {
        let (event_tx, mut event_rx) = mpsc::channel(1);
        poll_fetch_dag(
            self.transport.clone(),
            self.blockstore.clone(),
            event_tx,
            self.root,
            context,
            DagFetchLimiter::new(2),
            self.diagnostics.clone(),
        )
        .await;
        event_rx.recv().await
    }
}

fn peers(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[tokio::test(start_paused = true)]
async fn a_busy_reply_ends_the_window_at_once_as_busy_not_stalled() {
    let f = Fixture::new(&["remote-peer"], false).await;
    let started = n0_future::time::Instant::now();

    let outcome = poll_fetch_blocks(
        &f.root,
        &[f.child],
        &f.transport,
        &f.blockstore,
        &PeerId::new("remote-peer".to_string()),
        &f.alt_context(),
        ClosureDepth::initial(),
    )
    .await;

    assert_eq!(outcome, ProviderWindowOutcome::Busy);
    assert_eq!(n0_future::time::Instant::now(), started);
    assert_eq!(f.transport.cancelled_queries(), vec![1]);
}

#[tokio::test(start_paused = true)]
async fn a_busy_provider_is_rotated_past_without_spending_stall_budget() {
    let f = Fixture::new(&["remote-peer"], true).await;
    let context = f.alt_context();
    let mut providers = ProviderRotation::new(context.providers());
    let mut state = ProviderFetchState::new(&mut providers, None, f.diagnostics.as_ref());
    state.stall_budget = 1;

    let outcome = poll_fetch_blocks_rotating(
        &f.root,
        &[f.child],
        &f.transport,
        &f.blockstore,
        &mut state,
        &context,
    )
    .await;

    assert_eq!(outcome, FetchBatchOutcome::Complete);
    assert_eq!(
        (state.stall_budget, state.free_refusals),
        (1, MAX_FREE_REFUSALS),
        "a busy reply is neither a stalled window nor a refusal"
    );
    assert!(f.blockstore.has(&f.child).await.unwrap());
    assert_eq!(
        f.transport.sync_providers(),
        peers(&["remote-peer", "alt-peer"])
    );
    let snapshot = f.diagnostics.snapshot();
    assert_eq!(snapshot.provider_busy_rotations, 1);
    assert_eq!(snapshot.provider_rotations, 1);
}

#[tokio::test(start_paused = true)]
async fn every_provider_busy_defers_to_the_root_clock_without_exhaustion() {
    let f = Fixture::new(&["remote-peer", "alt-peer"], true).await;
    let started = n0_future::time::Instant::now();

    assert!(f.fetch_dag(f.alt_context()).await.is_none());

    assert_eq!(
        n0_future::time::Instant::now() - started,
        retry_backoff(2),
        "busy providers cost no fetch window, only the one busy retry's backoff"
    );
    assert_eq!(
        f.transport.sync_batches().len(),
        4,
        "one request per provider per pass: the attempt and its one busy retry"
    );
    let snapshot = f.diagnostics.snapshot();
    assert_eq!(snapshot.pending_dag_fetch_deferred_busy, 1);
    assert_eq!(snapshot.pending_dag_fetch_exhausted, 0);
    assert_eq!(snapshot.provider_busy_rotations, 4);
}

/// A busy hub whose alternate does not hold the fresh write yet and answers
/// empty: the attempt proves only saturation, so it must not exhaust the root.
#[tokio::test(start_paused = true)]
async fn a_busy_publisher_with_empty_alternates_defers_instead_of_exhausting() {
    let f = Fixture::new(&["remote-peer"], true).await;
    f.transport.set_failure_answer(
        f.completions.clone(),
        FetchFailureCause::NoBlocks,
        Duration::ZERO,
    );

    assert!(f.fetch_dag(f.alt_context()).await.is_none());

    let snapshot = f.diagnostics.snapshot();
    assert_eq!(snapshot.pending_dag_fetch_exhausted, 0);
    assert_eq!(snapshot.pending_dag_fetch_deferred_busy, 1);
    assert_eq!(
        f.transport.sync_providers(),
        peers(&["remote-peer", "alt-peer", "remote-peer", "alt-peer"]),
        "each pass asks the busy hub once and its empty alternate once"
    );
    assert!(f.transport.disconnected_peers().is_empty());
}

#[tokio::test(start_paused = true)]
async fn empty_providers_without_a_busy_reply_still_exhaust() {
    let f = Fixture::new(&[], true).await;
    f.transport.set_failure_answer(
        f.completions.clone(),
        FetchFailureCause::NoBlocks,
        Duration::ZERO,
    );

    f.fetch_dag(f.alt_context()).await;

    let snapshot = f.diagnostics.snapshot();
    assert_eq!(snapshot.pending_dag_fetch_exhausted, 1);
    assert_eq!(snapshot.pending_dag_fetch_deferred_busy, 0);
}

#[tokio::test(start_paused = true)]
async fn a_lone_busy_publisher_is_asked_once_per_pass_and_kept_connected() {
    let f = Fixture::new(&["remote-peer"], false).await;
    f.transport.set_busy_car_answer(f.rooted_car.clone());
    f.transport
        .set_connected_peers(vec![PeerId::new("remote-peer".to_string())]);
    let context = f.rooted_context();

    assert!(f.fetch_dag(context).await.is_none());

    assert_eq!(
        f.transport.car_request_count(),
        2,
        "one rooted CAR per pass"
    );
    assert!(
        f.transport.sync_batches().is_empty(),
        "the root-block batch skips the publisher that just shed"
    );
    let snapshot = f.diagnostics.snapshot();
    assert_eq!(snapshot.pending_dag_fetch_deferred_busy, 1);
    assert_eq!(snapshot.pending_dag_fetch_exhausted, 0);
    assert_eq!(snapshot.provider_busy_rotations, 2);
    assert!(f.transport.disconnected_peers().is_empty());
}

/// A busy answer must not mark the publisher answered: that is the liveness
/// evidence the dead-Bitswap hang-up needs.
#[tokio::test(start_paused = true)]
async fn a_busy_rooted_car_answer_is_busy_and_not_an_answer() {
    let f = Fixture::new(&[], false).await;
    f.transport.set_busy_car_answer(f.rooted_car.clone());
    let context = f.rooted_context();
    let mut answered = false;
    let started = n0_future::time::Instant::now();

    let outcome = poll_fetch_rooted_car(
        &f.root,
        std::slice::from_ref(&f.root),
        &f.transport,
        &f.blockstore,
        &PeerId::new("remote-peer".to_string()),
        &context,
        &mut answered,
    )
    .await;

    assert_eq!(outcome, ProviderWindowOutcome::Busy);
    assert!(!answered, "a busy publisher is alive, not silent");
    assert!(
        n0_future::time::Instant::now() - started < Duration::from_secs(1),
        "a busy answer does not wait out the 10 s rooted deadline"
    );
}

#[tokio::test(start_paused = true)]
async fn a_busy_rooted_window_rotates_to_the_alternate() {
    let f = Fixture::new(&[], false).await;
    f.transport.set_busy_car_answer(f.rooted_car.clone());
    let context = f
        .alt_context()
        .with_rooted_car_completions(f.rooted_car.clone());

    assert!(matches!(
        f.fetch_dag(context).await,
        Some(SyncEvent::DagReady { .. })
    ));

    assert_eq!(f.transport.car_request_count(), 1);
    assert_eq!(
        f.transport.sync_providers(),
        peers(&["alt-peer", "alt-peer"]),
        "the shed publisher is skipped for the root block and the walk"
    );
    let snapshot = f.diagnostics.snapshot();
    assert_eq!(snapshot.provider_busy_rotations, 1);
    assert_eq!(snapshot.pending_dag_fetch_deferred_busy, 0);
}

#[tokio::test(start_paused = true)]
async fn a_momentary_all_busy_moment_retries_within_the_dispatch() {
    let f = Fixture::new(&["remote-peer"], true).await;
    f.transport
        .set_connected_peers(vec![PeerId::new("remote-peer".to_string())]);
    let busy = f.transport.busy_providers.clone();
    n0_future::task::spawn(async move {
        n0_future::time::sleep(Duration::from_secs(1)).await;
        busy.remove("remote-peer");
    });
    let context = f.rooted_context();

    assert!(matches!(
        f.fetch_dag(context).await,
        Some(SyncEvent::DagReady { .. })
    ));

    assert!(f.blockstore.has(&f.child).await.unwrap());
    let snapshot = f.diagnostics.snapshot();
    assert_eq!(snapshot.pending_dag_fetch_deferred_busy, 0);
    assert_eq!(snapshot.pending_dag_fetch_exhausted, 0);
}
