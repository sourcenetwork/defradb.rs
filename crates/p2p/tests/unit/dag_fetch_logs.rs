//! How the DAG fetcher classifies and logs selective windows that made no
//! progress (#2524, #2530), checked through the emitted tracing events.

use super::*;
use crate::sync::manager::log_throttle::LOG_THROTTLE_WINDOW;
use kovan_queue::seg_queue::SegQueue;
use regolith::sync::Lazy;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

/// iroh's `REQUEST_RESPONSE_TIMEOUT`, which fires before the fetcher's watchdog.
const TRANSPORT_RESPONSE_DEADLINE: Duration = Duration::from_secs(30);
const EXHAUSTED: &str = "DAG fetch failed after exhausting retries";
const RETRY: &str = "DAG fetch incomplete, retrying after backoff";

#[derive(Debug)]
struct Line {
    level: Level,
    message: String,
    fields: Vec<(&'static str, String)>,
}

impl Line {
    fn field(&self, name: &str) -> Option<&str> {
        let (_, value) = self.fields.iter().find(|(field, _)| *field == name)?;
        Some(value)
    }
}

impl Visit for Line {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => self.message = format!("{value:?}"),
            name => self.fields.push((name, format!("{value:?}"))),
        }
    }
}

#[derive(Clone, Default)]
struct Capture(Arc<SegQueue<Line>>);

impl Capture {
    fn lines(&self) -> Vec<Line> {
        std::iter::from_fn(|| self.0.pop()).collect()
    }
}

impl Subscriber for Capture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        let mut line = Line {
            level: *event.metadata().level(),
            message: String::new(),
            fields: Vec::new(),
        };
        event.record(&mut line);
        self.0.push(line);
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

/// The summary task runs on the paused edge's worker thread, where a scoped
/// subscriber does not reach. These tests install one process-wide
/// subscriber instead, keeping only the events of that worker and of this
/// module's own tests, and take turns so one test's lines never reach the
/// other.
static SUMMARY_LINES: Lazy<Capture> = Lazy::new(|| {
    let capture = Capture::default();
    tracing::subscriber::set_global_default(OwnThreads(capture.clone()))
        .expect("no other test installs a process-wide subscriber");
    capture
});

static SUMMARY_TURN: Lazy<regolith::sync::Mutex<()>> = Lazy::new(|| regolith::sync::Mutex::new(()));

struct OwnThreads(Capture);

impl Subscriber for OwnThreads {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.0.enabled(metadata)
    }
    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        self.0.new_span(attributes)
    }
    fn record(&self, span: &Id, values: &Record<'_>) {
        self.0.record(span, values);
    }
    fn record_follows_from(&self, span: &Id, follows: &Id) {
        self.0.record_follows_from(span, follows);
    }
    fn event(&self, event: &Event<'_>) {
        let thread = std::thread::current();
        let own = thread.name().is_some_and(|name| {
            name == "defra-edge-paused-test" || name.contains("::dag_fetch_logs::")
        });
        if own {
            self.0.event(event);
        }
    }
    fn enter(&self, span: &Id) {
        self.0.enter(span);
    }
    fn exit(&self, span: &Id) {
        self.0.exit(span);
    }
}

/// The captured lines once `errors` of them are at ERROR. The summary task
/// runs on the edge's worker and may register its timer after the test's
/// own, so the paused clock is advanced a second at a time, yielding to the
/// worker between steps, under a real deadline.
async fn lines_with_errors(capture: &Capture, errors: usize) -> Vec<Line> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut lines = Vec::new();
    while std::time::Instant::now() < deadline {
        lines.extend(capture.lines());
        if lines
            .iter()
            .filter(|line| line.level == Level::ERROR)
            .count()
            >= errors
        {
            break;
        }
        nuclei::time::advance(Duration::from_secs(1)).await;
        crate::rt::yield_now().await;
    }
    lines
}

struct Fetch {
    transport: TestTransport,
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    root_cid: Cid,
    child_cid: Cid,
    context: DagFetchContext,
}

/// A one-child DAG whose provider answers every selective query with a
/// failure of `cause` after `after`, or never answers its first one.
fn fetch(answer: Option<(FetchFailureCause, Duration)>) -> Fetch {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (root_cid, root_data, child_cid, child_data) = single_child_dag();
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(child_cid, child_data)]),
    );
    let completion = crate::sync::manager::BlockSyncCompletionTracker::default();
    match answer {
        Some((cause, after)) => transport.set_failure_answer(completion.clone(), cause, after),
        None => transport.set_skip_serving_syncs(1),
    }
    let context = DagFetchContext::new(
        "doc-id".to_string(),
        "collection-id".to_string(),
        "creator-id".to_string(),
        PeerId::new("remote-peer".to_string()),
    )
    .with_block_sync_completions(completion);
    Fetch {
        transport,
        blockstore,
        root_cid,
        child_cid,
        context,
    }
}

fn refused() -> Fetch {
    fetch(Some((FetchFailureCause::Refused, Duration::ZERO)))
}

fn transport_timeout() -> Fetch {
    fetch(Some((
        FetchFailureCause::TimedOut,
        TRANSPORT_RESPONSE_DEADLINE,
    )))
}

async fn fetch_until_exhausted(fetch: Fetch, diagnostics: &Arc<SyncDiagnostics>) {
    let (event_tx, _event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        fetch.transport,
        fetch.blockstore,
        event_tx,
        fetch.root_cid,
        fetch.context,
        DagFetchLimiter::new(4),
        Arc::clone(diagnostics),
    )
    .await;
}

#[nuclei::test(start_paused = true)]
async fn each_window_ending_is_classified_by_what_ended_it() {
    let superseded = {
        let fetch = fetch(None);
        let (manager, _events) = crate::sync::manager::SyncManager::new(
            fetch.blockstore.clone(),
            Arc::new(crate::sync::peer_state::PeerStateTracker::new()),
            crate::sync::manager::SyncConfig::default(),
        );
        let context = fetch
            .context
            .clone()
            .with_pending_lease(manager.pending_dag_lease(fetch.root_cid));
        assert!(!context.is_current());
        Fetch { context, ..fetch }
    };
    let cases = [
        (
            fetch(None),
            ProviderWindowOutcome::Stalled,
            BLOCK_SYNC_COMPLETION_WATCHDOG,
        ),
        (
            superseded,
            ProviderWindowOutcome::Superseded,
            Duration::ZERO,
        ),
        (
            transport_timeout(),
            ProviderWindowOutcome::Failed(FetchFailureCause::TimedOut),
            TRANSPORT_RESPONSE_DEADLINE,
        ),
    ];
    for (fetch, expected, ends_after) in cases {
        let started = crate::rt::time::Instant::now();
        let outcome = poll_fetch_blocks(
            &fetch.root_cid,
            &[fetch.child_cid],
            &fetch.transport,
            &fetch.blockstore,
            &PeerId::new("hub".to_string()),
            &fetch.context,
            ClosureDepth::initial(),
        )
        .await;

        assert_eq!(outcome, expected);
        assert_eq!(
            started.elapsed().as_secs(),
            ends_after.as_secs(),
            "{expected:?}"
        );
    }
}

/// Fast refusals rotate at once and spend the attempt's free refusals, not its
/// stall budget, and are still reported as refused.
#[nuclei::test(start_paused = true)]
async fn refusing_providers_rotate_and_are_reported_as_refused() {
    let fetch = refused();
    let mut providers = ProviderRotation::new(peers(&["hub", "spoke"]));
    let diagnostics = diagnostics();
    let started = crate::rt::time::Instant::now();
    let mut state = ProviderFetchState::new(&mut providers, None, diagnostics.as_ref());

    let outcome = poll_fetch_blocks_rotating(
        &fetch.root_cid,
        &[fetch.child_cid],
        &fetch.transport,
        &fetch.blockstore,
        &mut state,
        &fetch.context,
    )
    .await;

    let FetchBatchOutcome::NoProgress(failures) = outcome else {
        panic!("expected no progress, got {outcome:?}");
    };
    assert_eq!(failures.provider_outcomes(), "hub:refused,spoke:refused");
    assert_eq!(
        (
            state.stall_budget,
            state.free_refusals,
            diagnostics.snapshot().provider_rotations
        ),
        (2, MAX_FREE_REFUSALS - 2, 2)
    );
    assert_eq!(crate::rt::time::Instant::now(), started);
}

/// Each failure kind gets its own full line per window, refusals are
/// throttled rather than hidden, the exhausted line keeps the whole provider
/// set, and the next full line reports what the throttle demoted.
#[nuclei::test(start_paused = true)]
async fn fetch_lines_name_their_cause_and_are_throttled_per_kind() {
    let capture = Capture::default();
    let _guard = tracing::subscriber::set_default(capture.clone());
    let diagnostics = diagnostics();

    futures::join!(
        fetch_until_exhausted(transport_timeout(), &diagnostics),
        fetch_until_exhausted(refused(), &diagnostics),
        fetch_until_exhausted(refused(), &diagnostics),
    );
    let lines = capture.lines();

    assert_eq!(diagnostics.snapshot().pending_dag_fetch_exhausted, 3);

    let exhausted: Vec<_> = lines
        .iter()
        .filter(|line| line.message.starts_with(EXHAUSTED))
        .collect();
    let full: Vec<_> = exhausted
        .iter()
        .filter(|line| line.level == Level::ERROR)
        .collect();
    assert_eq!((exhausted.len(), full.len()), (3, 2));
    assert!(full
        .iter()
        .any(|line| line.field("provider_outcomes") == Some("remote-peer:timed_out")));
    assert!(full.iter().all(
        |line| line.field("providers") == Some("[PeerId(\"remote-peer\")]")
            && line.field("provider_count") == Some("1")
    ));

    let count = |level: Level, message: &str| {
        lines
            .iter()
            .filter(|line| line.level == level && line.message.contains(message))
            .count()
    };
    assert!(count(Level::WARN, "providers did not answer") >= 1);
    assert_eq!(count(Level::WARN, "providers refused it"), 1);
    assert!(count(Level::DEBUG, "providers refused it") >= 1);
    assert!(lines.iter().any(|line| line.level == Level::WARN
        && line.message == RETRY
        && line.field("refused").is_some_and(|count| count != "0")));

    nuclei::time::advance(LOG_THROTTLE_WINDOW).await;
    fetch_until_exhausted(refused(), &diagnostics).await;
    let next = capture
        .lines()
        .into_iter()
        .find(|line| line.message.starts_with(EXHAUSTED))
        .expect("the next refused root logs its exhaustion");
    assert_eq!(next.level, Level::ERROR);
    assert_eq!(next.field("suppressed_since_last"), Some("1"));
}

/// A burst against one refusing provider followed by quiet still reports
/// the burst's volume once its window closes.
#[test]
fn an_exhaustion_burst_then_quiet_logs_one_full_line_and_one_summary() {
    edge::testing::paused(|edge| async move {
        let _turn = SUMMARY_TURN.lock().await;
        let capture = SUMMARY_LINES.clone();
        capture.lines();
        let diagnostics = diagnostics();
        diagnostics
            .dag_fetch_logs()
            .use_network(crate::network::Network::on(edge));

        futures::join!(
            fetch_until_exhausted(refused(), &diagnostics),
            fetch_until_exhausted(refused(), &diagnostics),
            fetch_until_exhausted(refused(), &diagnostics),
            fetch_until_exhausted(refused(), &diagnostics),
        );
        let lines = lines_with_errors(&capture, 2).await;

        assert_eq!(diagnostics.snapshot().pending_dag_fetch_exhausted, 4);
        let errors: Vec<_> = lines
            .iter()
            .filter(|line| line.level == Level::ERROR)
            .map(|line| line.message.as_str())
            .collect();
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].starts_with(EXHAUSTED));
        assert_eq!(
            errors[1],
            "4 roots exhausted against remote-peer in last 60s: refused=4"
        );
    })
}

/// A lone exhaustion the throttle demoted, against a provider set of its own,
/// still reaches ERROR through that set's summary.
#[test]
fn a_demoted_lone_exhaustion_is_summarized_for_its_provider_set() {
    edge::testing::paused(|edge| async move {
        let _turn = SUMMARY_TURN.lock().await;
        let capture = SUMMARY_LINES.clone();
        capture.lines();
        let diagnostics = diagnostics();
        diagnostics
            .dag_fetch_logs()
            .use_network(crate::network::Network::on(edge));
        let mut elsewhere = refused();
        elsewhere.context.source_peer = PeerId::new("other-peer".to_string());

        fetch_until_exhausted(refused(), &diagnostics).await;
        fetch_until_exhausted(elsewhere, &diagnostics).await;

        let errors: Vec<_> = lines_with_errors(&capture, 2)
            .await
            .into_iter()
            .filter(|line| line.level == Level::ERROR)
            .map(|line| line.message)
            .collect();
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].starts_with(EXHAUSTED));
        assert_eq!(
            errors[1],
            "1 roots exhausted against other-peer in last 60s: refused=1"
        );
    })
}

/// A fetch the per-root clock dispatched makes one attempt, so a stalled
/// root comes back to the fetcher once per attempt. Terminal exhaustion (the
/// counter, the ERROR line, the summary and the dead-Bitswap hang-up) still
/// happens once per `MAX_FETCH_ATTEMPTS` incomplete attempts of the root.
#[nuclei::test(start_paused = true)]
async fn clock_dispatches_report_exhaustion_once_per_attempt_budget() {
    use crate::sync::pending_store::{PendingDagStorage, PendingDagStore, PersistedPendingDag};

    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let (transport, root_cid, context) = dead_bitswap_publisher_fetch(&blockstore).await;
    let (manager, _events) = crate::sync::manager::SyncManager::new(
        blockstore.clone(),
        Arc::new(crate::sync::peer_state::PeerStateTracker::new()),
        crate::sync::manager::SyncConfig::default(),
    );
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));
    pending_store
        .put(
            &root_cid,
            &PersistedPendingDag {
                doc_id: "doc-id".to_string(),
                collection_id: "collection-id".to_string(),
                head_priority: None,
                creator: "creator-id".to_string(),
                source_peer: Some("dead-peer".to_string()),
                alternate_providers: vec!["alt-peer".to_string()],
                is_explicit_replicator: false,
                explicit_replay_authorization: None,
            },
        )
        .await
        .unwrap();
    manager.install_pending_dag_store(pending_store).await;
    assert_eq!(manager.resync_persisted_pending_dags().await, 1);
    let diagnostics = diagnostics();

    for dispatch in 1..=2 * MAX_FETCH_ATTEMPTS {
        let (event_tx, _event_rx) = mpsc::channel(1);
        poll_fetch_dag(
            transport.clone(),
            blockstore.clone(),
            event_tx,
            root_cid,
            context
                .clone()
                .with_pending_lease(manager.pending_dag_lease(root_cid)),
            DagFetchLimiter::new(2),
            diagnostics.clone(),
        )
        .await;
        assert_eq!(
            diagnostics.snapshot().pending_dag_fetch_exhausted,
            u64::from(dispatch / MAX_FETCH_ATTEMPTS),
            "after clock dispatch {dispatch}"
        );
    }

    assert_eq!(
        transport.disconnected_peers(),
        vec!["dead-peer".to_string(); 2],
        "the publisher is hung up on once per exhausted budget"
    );
}
