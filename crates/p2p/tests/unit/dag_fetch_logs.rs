//! How the DAG fetcher classifies and logs selective windows that made no
//! progress (#2524, #2530), checked through the emitted tracing events.

use super::*;
use crate::sync::manager::log_throttle::LOG_THROTTLE_WINDOW;
use kovan_queue::seg_queue::SegQueue;
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

#[tokio::test(start_paused = true)]
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
        let started = n0_future::time::Instant::now();
        let outcome = poll_fetch_blocks(
            &fetch.root_cid,
            &[fetch.child_cid],
            &fetch.transport,
            &fetch.blockstore,
            &PeerId::new("hub".to_string()),
            &fetch.context,
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

/// Refusing providers rotate and spend stall budget exactly as stalled ones.
#[tokio::test(start_paused = true)]
async fn refusing_providers_rotate_and_are_reported_as_refused() {
    let fetch = refused();
    let mut providers = ProviderRotation::new(peers(&["hub", "spoke"]));
    let mut stall_budget = providers.len();
    let diagnostics = diagnostics();
    let started = n0_future::time::Instant::now();

    let outcome = poll_fetch_blocks_rotating(
        &fetch.root_cid,
        &[fetch.child_cid],
        &fetch.transport,
        &fetch.blockstore,
        &mut ProviderFetchState {
            providers: &mut providers,
            stall_budget: &mut stall_budget,
            diagnostics: diagnostics.as_ref(),
        },
        &fetch.context,
    )
    .await;

    let FetchBatchOutcome::NoProgress(failures) = outcome else {
        panic!("expected no progress, got {outcome:?}");
    };
    assert_eq!(failures.provider_outcomes(), "hub:refused,spoke:refused");
    assert_eq!(
        (stall_budget, diagnostics.snapshot().provider_rotations),
        (0, 2)
    );
    assert_eq!(n0_future::time::Instant::now(), started);
}

/// Each failure kind gets its own full line per window, refusals are
/// throttled rather than hidden, the exhausted line keeps the whole provider
/// set, and the next full line reports what the throttle demoted.
#[tokio::test(start_paused = true)]
async fn fetch_lines_name_their_cause_and_are_throttled_per_kind() {
    let capture = Capture::default();
    let _guard = tracing::subscriber::set_default(capture.clone());
    let diagnostics = diagnostics();

    tokio::join!(
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

    tokio::time::advance(LOG_THROTTLE_WINDOW).await;
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
#[tokio::test(start_paused = true)]
async fn an_exhaustion_burst_then_quiet_logs_one_full_line_and_one_summary() {
    let capture = Capture::default();
    let _guard = tracing::subscriber::set_default(capture.clone());
    let diagnostics = diagnostics();

    tokio::join!(
        fetch_until_exhausted(refused(), &diagnostics),
        fetch_until_exhausted(refused(), &diagnostics),
        fetch_until_exhausted(refused(), &diagnostics),
        fetch_until_exhausted(refused(), &diagnostics),
    );
    tokio::time::sleep(LOG_THROTTLE_WINDOW + Duration::from_secs(1)).await;
    let lines = capture.lines();

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
}

/// A lone exhaustion the throttle demoted, against a provider set of its own,
/// still reaches ERROR through that set's summary.
#[tokio::test(start_paused = true)]
async fn a_demoted_lone_exhaustion_is_summarized_for_its_provider_set() {
    let capture = Capture::default();
    let _guard = tracing::subscriber::set_default(capture.clone());
    let diagnostics = diagnostics();
    let mut elsewhere = refused();
    elsewhere.context.source_peer = PeerId::new("other-peer".to_string());

    fetch_until_exhausted(refused(), &diagnostics).await;
    fetch_until_exhausted(elsewhere, &diagnostics).await;
    tokio::time::sleep(LOG_THROTTLE_WINDOW + Duration::from_secs(1)).await;

    let errors: Vec<_> = capture
        .lines()
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
}
