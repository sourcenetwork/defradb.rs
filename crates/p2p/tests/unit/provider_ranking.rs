use super::*;
use crate::sync::coordinator::dag_retry::MAX_FREE_REFUSALS;
use crate::sync::manager::BlockSyncCompletionTracker;
use crate::transport::PeerFetchStats;

const SOURCE: &str = "remote-peer";
const ALT: &str = "alt-peer";

fn measured(peer: &str, fetches: u64, success_ppm: u64) -> PeerFetchStats {
    PeerFetchStats {
        peer: peer.to_string(),
        fetches,
        success_ppm,
        ..Default::default()
    }
}

fn context(alternate: bool) -> DagFetchContext {
    let context = DagFetchContext::new(
        "doc-id".to_string(),
        "collection-id".to_string(),
        "creator-id".to_string(),
        PeerId::new(SOURCE.to_string()),
    );
    if alternate {
        context.with_alternate_providers(vec![PeerId::new(ALT.to_string())])
    } else {
        context
    }
}

struct Fixture {
    transport: TestTransport,
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    root_cid: Cid,
}

impl Fixture {
    /// A root with `children` links, of which only the selective path serves
    /// the first `served`. `cancellable` picks iroh's rooted sync (ranked) or
    /// libp2p's (not).
    async fn new(children: usize, served: usize, cancellable: bool) -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store, true));
        let blocks: Vec<(Cid, Vec<u8>)> = (0..children)
            .map(|i| {
                let data = encode_ipld(ipld!({ "value": i as i64 }));
                (make_cid(&data), data)
            })
            .collect();
        let root_data = encode_ipld(Ipld::List(
            blocks.iter().map(|(cid, _)| Ipld::Link(*cid)).collect(),
        ));
        let root_cid = make_cid(&root_data);
        blockstore.put(&root_cid, &root_data).await.unwrap();
        let transport = TestTransport::new(
            blockstore.clone(),
            root_cid,
            root_data,
            RapidHashMap::new(),
            RapidHashMap::from_iter(blocks.into_iter().take(served)),
        );
        if cancellable {
            transport.set_force_rooted_sync();
        }
        Self {
            transport,
            blockstore,
            root_cid,
        }
    }

    /// Makes `peer` fail every selective query `context` sends after `delay`.
    fn refuse(&self, peer: &str, delay: Duration, context: DagFetchContext) -> DagFetchContext {
        let completions = BlockSyncCompletionTracker::default();
        self.transport
            .refusing_providers
            .insert(peer.to_string(), (completions.clone(), delay));
        context.with_block_sync_completions(completions)
    }

    async fn fetch(
        &self,
        context: DagFetchContext,
        limiter: &DagFetchLimiter,
        diagnostics: &Arc<SyncDiagnostics>,
    ) -> bool {
        let (event_tx, mut event_rx) = mpsc::channel(1);
        poll_fetch_dag(
            self.transport.clone(),
            self.blockstore.clone(),
            event_tx,
            self.root_cid,
            context,
            limiter.clone(),
            diagnostics.clone(),
        )
        .await;
        matches!(
            event_rx.next().await,
            Some(SyncEvent::DagReady { root_cid, .. }) if root_cid == self.root_cid
        )
    }
}

#[nuclei::test(start_paused = true)]
async fn a_failing_source_is_asked_after_a_healthy_alternate() {
    // (source success_ppm, source dead, asked, demotions)
    for (source_ppm, dead, asked, demotions) in
        [(20_897, true, ALT, 1), (1_000_000, false, SOURCE, 0)]
    {
        let fixture = Fixture::new(1, 1, true).await;
        if dead {
            fixture.transport.mark_provider_dead(SOURCE);
        }
        fixture.transport.fetch_stats.store(vec![
            measured(SOURCE, 82_928, source_ppm),
            measured(ALT, 103, 1_000_000),
        ]);
        let diagnostics = diagnostics();
        let started = crate::rt::time::Instant::now();

        assert!(
            fixture
                .fetch(context(true), &DagFetchLimiter::new(2), &diagnostics)
                .await
        );

        assert_eq!(fixture.transport.sync_providers(), [asked]);
        assert!(started.elapsed() < BLOCK_SYNC_COMPLETION_WATCHDOG);
        assert_eq!(diagnostics.snapshot().provider_demotions, demotions);
    }
}

/// Only the announcing peer's fast refusal cools it for the next root: an
/// alternate may just not hold a root published moments ago, and on libp2p the
/// source keeps the rooted CAR slot the dead-Bitswap repair depends on.
#[nuclei::test(start_paused = true)]
async fn a_fast_refusal_cools_only_the_announcing_peer_on_cancellable_transports() {
    let cases = [
        (SOURCE, true, vec![], [&[SOURCE, ALT][..], &[ALT]], 1),
        (
            ALT,
            true,
            vec![measured(SOURCE, 82_928, 20_897)],
            [&[ALT, SOURCE], &[ALT, SOURCE]],
            0,
        ),
        (SOURCE, false, vec![], [&[SOURCE, ALT], &[SOURCE, ALT]], 0),
    ];
    for (refusing, cancellable, stats, expected, cooldown_skips) in cases {
        let limiter = DagFetchLimiter::new(2);
        let diagnostics = diagnostics();
        for expected in expected {
            let fixture = Fixture::new(1, 1, cancellable).await;
            fixture.transport.fetch_stats.store(stats.clone());
            let context = fixture.refuse(refusing, Duration::ZERO, context(true));
            assert!(fixture.fetch(context, &limiter, &diagnostics).await);
            assert_eq!(fixture.transport.sync_providers(), expected);
        }
        assert_eq!(
            diagnostics.snapshot().provider_cooldown_skips,
            cooldown_skips,
            "{refusing} refusing, cancellable={cancellable}"
        );
    }
}

/// Fetches a root of `batches` unservable selective batches from a source
/// that fails each one after `delay`; returns the batches asked, the time it
/// took and the limiter it ran under.
async fn refused_batches(batches: usize, delay: Duration) -> (usize, Duration, DagFetchLimiter) {
    let fixture = Fixture::new(SELECTIVE_FETCH_BATCH_SIZE * (batches - 1) + 1, 0, true).await;
    let context = fixture.refuse(SOURCE, delay, context(false));
    let limiter = DagFetchLimiter::new(2);
    let started = crate::rt::time::Instant::now();
    assert!(!fixture.fetch(context, &limiter, &diagnostics()).await);
    (
        fixture.transport.sync_batches().len(),
        started.elapsed(),
        limiter,
    )
}

#[nuclei::test(start_paused = true)]
async fn a_fast_refusal_does_not_spend_the_stall_budget() {
    let (asked, _, _) = refused_batches(2, Duration::ZERO).await;
    assert_eq!(asked, 2 * MAX_FETCH_ATTEMPTS as usize);
}

/// A failure only at iroh's 30 s response timeout waited the window out.
#[nuclei::test(start_paused = true)]
async fn a_failure_at_the_response_timeout_spends_the_stall_budget() {
    let (asked, elapsed, limiter) = refused_batches(2, Duration::from_secs(30)).await;
    assert_eq!(asked, MAX_FETCH_ATTEMPTS as usize);
    assert!(elapsed < Duration::from_secs(100), "{elapsed:?}");
    let next = limiter.provider_health().rank(
        vec![
            PeerId::new(SOURCE.to_string()),
            PeerId::new(ALT.to_string()),
        ],
        &[],
        crate::rt::time::Instant::now(),
    );
    assert_eq!(next.cooling, 0, "a waited-out failure starts no cool-down");
}

#[nuclei::test(start_paused = true)]
async fn free_refusals_are_capped_per_attempt() {
    let (asked, _, _) = refused_batches(MAX_FREE_REFUSALS + 3, Duration::ZERO).await;
    assert_eq!(asked, MAX_FETCH_ATTEMPTS as usize * (MAX_FREE_REFUSALS + 1));
}
