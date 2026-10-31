//! A selected push stops after its first stall, lost connection, pause hint
//! or caller stop, bounded by the work already started
//! (`SelectedPushIsolation.tla`, `DocumentPush.lean` `serialRun`).

use super::push_docs_replay::{ConnectedPeerTransport, ReplyScript};
use db::merge::push_docs_replay::{
    DEFAULT_MAX_CONCURRENT_REPLAY_SENDS, DEFAULT_REPLAY_RATE_LIMIT_BURST,
    DEFAULT_REPLAY_RATE_LIMIT_RATE,
};
use db::merge::{
    document_heads_acknowledged, push_existing_docs_by_id, ReplayPushConfig, SelectedPushStop,
};
use db::{AutoCommitMutator, DB};
use document::Document;
use p2p::message::PushLogReply;
use p2p::transport::PeerId;
use query::mutator::DocMutator;
use rapidhash::{HashSetExt, RapidHashSet};
use schema::{CollectionVersion, FieldDescription, FieldKind};
use std::sync::Arc;
use std::time::{Duration, Instant};
use storage::RegolithStore;

struct Fixture {
    db: Arc<DB<RegolithStore>>,
    selected: Vec<(String, String)>,
    peer: PeerId,
    schedule_before: Option<bytes::Bytes>,
}

impl Fixture {
    /// `selected` documents to send and `unselected` ones that must stay home,
    /// with a subscription to the peer that is in a long backoff.
    async fn new(selected: usize, unselected: usize) -> Self {
        let db = Arc::new(DB::from_arc(Arc::new(RegolithStore::in_memory().unwrap())).unwrap());
        db.set_network(edge::testing::edge());
        db.create_collection(CollectionVersion::new(
            "Note",
            "v1",
            "notes",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new("2", "body", FieldKind::string(), schema::CType::LwwRegister),
            ],
        ))
        .await
        .unwrap();
        let mutator = AutoCommitMutator::new(db.clone());
        let schema = db.get_collection("Note").unwrap().unwrap().schema_arc();
        let mut ids = Vec::new();
        for index in 0..selected + unselected {
            let mut doc = Document::new(schema.clone());
            doc.set("body", format!("note-{index}")).unwrap();
            ids.push(
                mutator
                    .create("Note", doc)
                    .await
                    .unwrap()
                    .doc_id
                    .to_string(),
            );
        }
        let peer = PeerId::new("receiver".into());
        let info =
            p2p::ReplicatorInfo::from_raw(peer.as_str().to_owned(), vec!["notes".into()], vec![]);
        let peerstore = db.peerstore();
        peerstore
            .create_replicator(peer.as_str(), &info.to_bytes().unwrap())
            .await
            .unwrap();
        peerstore
            .record_retry_after(peer.as_str(), Duration::from_secs(600))
            .await
            .unwrap();
        let schedule_before = peerstore.get_retry_info(peer.as_str()).await.unwrap();
        assert!(schedule_before.is_some());
        let selected = ids[..selected]
            .iter()
            .map(|id| ("Note".to_owned(), id.clone()))
            .collect();
        Self {
            db,
            selected,
            peer,
            schedule_before,
        }
    }

    /// A receiver that answers every PushLog it gets, with `script` deciding
    /// the replies it overrides.
    fn receiver(
        &self,
        script: Option<ReplyScript>,
    ) -> (
        ConnectedPeerTransport,
        regolith::sync::BoundedReceiver<p2p::PushLogRequest>,
    ) {
        let (sent, received) = regolith::sync::bounded(4096);
        let mut transport =
            ConnectedPeerTransport::new("sender", self.peer.as_str()).with_replies(true, sent);
        if let Some(script) = script {
            transport = transport.with_reply_script(script);
        }
        (transport, received)
    }

    async fn push(
        &self,
        transport: &ConnectedPeerTransport,
        config: ReplayPushConfig,
        stop: &SelectedPushStop,
    ) -> Result<(), String> {
        push_existing_docs_by_id(
            transport,
            &self.db,
            None,
            &self.peer,
            &self.selected,
            &p2p::ReplicationFilters::new(),
            Default::default(),
            config,
            &p2p::EqOnlyFilterMatcher,
            &p2p::sync::HeadHintCarAuthority::default(),
            stop,
        )
        .await
    }

    async fn acknowledged(&self) -> usize {
        let ids: Vec<_> = self.selected.iter().map(|(_, id)| id.clone()).collect();
        document_heads_acknowledged(&self.db, self.peer.as_str(), "Note", &ids)
            .await
            .unwrap()
            .into_iter()
            .filter(|&acknowledged| acknowledged)
            .count()
    }

    /// How many selected documents the call prepared: it persisted serving
    /// authority for their heads, which it does just before starting them.
    async fn prepared(&self) -> usize {
        let peerstore = self.db.peerstore();
        let mut prepared = 0;
        for (_, doc_id) in &self.selected {
            let heads = db::merge::load_document_head_blocks(&self.db, doc_id)
                .await
                .unwrap();
            let mut granted = !heads.is_empty();
            for (cid, _) in heads {
                granted &= peerstore
                    .selected_car_root_allowed(self.peer.as_str(), &cid.to_bytes())
                    .await
                    .unwrap();
            }
            prepared += usize::from(granted);
        }
        prepared
    }

    /// Only selected documents crossed, each once; returns the ones that did.
    fn assert_only_selected_sent(
        &self,
        received: &regolith::sync::BoundedReceiver<p2p::PushLogRequest>,
    ) -> RapidHashSet<String> {
        let selected: RapidHashSet<_> = self.selected.iter().map(|(_, id)| id.clone()).collect();
        let mut sent = RapidHashSet::new();
        while let Some(request) = received.try_recv() {
            assert!(
                selected.contains(&request.doc_id),
                "unselected {} crossed",
                request.doc_id
            );
            assert!(
                sent.insert(request.doc_id.clone()),
                "{} sent twice",
                request.doc_id
            );
        }
        sent
    }

    /// The subscription's retry schedule and markers are what they were.
    async fn assert_subscription_untouched(&self) {
        let peerstore = self.db.peerstore();
        assert_eq!(
            self.schedule_before,
            peerstore.get_retry_info(self.peer.as_str()).await.unwrap()
        );
        assert!(peerstore
            .get_retry_documents(self.peer.as_str())
            .await
            .unwrap()
            .is_empty());
    }
}

fn never_replies() -> ReplyScript {
    Arc::new(|_| Box::pin(futures::future::pending()))
}

fn pause_hint() -> PushLogReply {
    let mut reply = PushLogReply::error("selected-transfer", p2p::error::RATE_LIMITED_MESSAGE);
    reply.retry_after_ms = Some(45_000);
    reply
}

#[test]
fn a_paced_selection_longer_than_ten_seconds_finishes_in_one_call() {
    workers::block_on(async {
        let fixture = Fixture::new(1200, 0).await;
        let (transport, received) = fixture.receiver(None);
        let started = Instant::now();
        fixture
            .push(&transport, Default::default(), &SelectedPushStop::new())
            .await
            .unwrap();
        let elapsed = started.elapsed();
        let burst = f64::from(DEFAULT_REPLAY_RATE_LIMIT_BURST);
        let paced = Duration::from_secs_f64((1200.0 - burst) / DEFAULT_REPLAY_RATE_LIMIT_RATE);
        assert!(paced > Duration::from_secs(10));
        assert!(
            elapsed + Duration::from_millis(100) >= paced,
            "{elapsed:?} beat the receiver's own token bucket ({paced:?})"
        );
        assert_eq!(fixture.assert_only_selected_sent(&received).len(), 1200);
        let arrivals = transport.arrivals();
        assert_eq!(arrivals.len(), 1200);
        // Arrival jitter is bounded by the sends in flight.
        let slack = DEFAULT_MAX_CONCURRENT_REPLAY_SENDS as f64;
        for (index, at) in arrivals.iter().enumerate() {
            let since = at.duration_since(arrivals[0]).as_secs_f64();
            assert!(
                (index + 1) as f64 <= burst + DEFAULT_REPLAY_RATE_LIMIT_RATE * since + slack,
                "PushLog {index} arrived {since}s in, over the receiver's rate limit"
            );
        }
        assert_eq!(fixture.acknowledged().await, 1200);
        fixture.assert_subscription_untouched().await;
    })
    .unwrap();
}

#[test]
fn a_silent_peer_costs_one_deadline_per_send_slot() {
    workers::block_on(async {
        let fixture = Fixture::new(200, 20).await;
        let (transport, received) = fixture.receiver(Some(never_replies()));
        let deadline = Duration::from_millis(250);
        let started = Instant::now();
        let error = fixture
            .push(
                &transport,
                ReplayPushConfig {
                    send_timeout: deadline,
                    ..Default::default()
                },
                &SelectedPushStop::new(),
            )
            .await
            .unwrap_err();
        let elapsed = started.elapsed();
        assert!(
            error.contains("stopped: replay PushLog timed out"),
            "{error}"
        );
        assert!(error.contains("(0 of 200 acknowledged)"), "{error}");
        let attempted = transport.arrivals().len();
        assert!(
            (1..=DEFAULT_MAX_CONCURRENT_REPLAY_SENDS).contains(&attempted),
            "{attempted} PushLogs went to a silent peer"
        );
        // Without the stop the call pays one deadline per eight documents (25).
        assert!(
            elapsed < deadline * 8,
            "a silent peer held the call {elapsed:?}"
        );
        fixture.assert_only_selected_sent(&received);
        assert_eq!(fixture.acknowledged().await, 0);
        // The started tasks and the one the scheduler was preparing: never
        // the rest of the selection.
        let document_slots = ReplayPushConfig::default().max_concurrent_document_tasks;
        assert!(fixture.prepared().await <= document_slots + 1);
        fixture.assert_subscription_untouched().await;
    })
    .unwrap();
}

#[test]
fn a_lost_connection_stops_and_the_next_call_sends_every_selected_head_again() {
    workers::block_on(async {
        let fixture = Fixture::new(120, 40).await;
        let reachable = 30;
        let (dropping, received) = fixture.receiver(Some(Arc::new(move |index| {
            Box::pin(
                async move { (index >= reachable).then_some(Err(p2p::Error::ConnectionClosed)) },
            )
        })));
        let error = fixture
            .push(&dropping, Default::default(), &SelectedPushStop::new())
            .await
            .unwrap_err();
        assert!(error.contains("stopped: "), "{error}");
        assert!(error.contains("(30 of 120 acknowledged)"), "{error}");
        let attempted = dropping.arrivals().len();
        assert!(
            attempted <= reachable + DEFAULT_MAX_CONCURRENT_REPLAY_SENDS,
            "{attempted} PushLogs after the connection dropped at {reachable}"
        );
        fixture.assert_only_selected_sent(&received);
        assert_eq!(fixture.acknowledged().await, reachable);
        fixture.assert_subscription_untouched().await;

        // An independent call proves every current head again.
        let (healthy, received) = fixture.receiver(None);
        fixture
            .push(&healthy, Default::default(), &SelectedPushStop::new())
            .await
            .unwrap();
        assert_eq!(healthy.arrivals().len(), 120);
        assert_eq!(fixture.assert_only_selected_sent(&received).len(), 120);
        assert_eq!(fixture.acknowledged().await, 120);
        fixture.assert_subscription_untouched().await;
    })
    .unwrap();
}

#[test]
fn a_receiver_pause_hint_stops_the_call_and_stays_with_it() {
    workers::block_on(async {
        let fixture = Fixture::new(64, 0).await;
        let (transport, received) = fixture.receiver(Some(Arc::new(|_| {
            Box::pin(async { Some(Ok(pause_hint())) })
        })));
        let error = fixture
            .push(&transport, Default::default(), &SelectedPushStop::new())
            .await
            .unwrap_err();
        assert!(error.contains("stopped: receiver backpressure"), "{error}");
        assert!(transport.arrivals().len() <= DEFAULT_MAX_CONCURRENT_REPLAY_SENDS);
        fixture.assert_only_selected_sent(&received);
        fixture.assert_subscription_untouched().await;
    })
    .unwrap();
}

#[test]
fn a_raised_stop_ends_scheduling_and_keeps_what_was_acknowledged() {
    workers::block_on(async {
        let fixture = Fixture::new(200, 0).await;
        let stop = SelectedPushStop::new();
        let raise = stop.clone();
        let raised_at = 10;
        let (transport, received) = fixture.receiver(Some(Arc::new(move |index| {
            if index == raised_at {
                raise.stop("the test gave up");
            }
            Box::pin(async { None })
        })));
        let error = fixture
            .push(&transport, Default::default(), &stop)
            .await
            .unwrap_err();
        assert!(error.contains("stopped: the test gave up"), "{error}");
        let attempted = transport.arrivals().len();
        // The raising send and the seven others that already held a send slot.
        assert!(
            attempted <= raised_at + DEFAULT_MAX_CONCURRENT_REPLAY_SENDS,
            "{attempted} PushLogs after the stop at {raised_at}"
        );
        assert_eq!(
            fixture.assert_only_selected_sent(&received).len(),
            attempted
        );
        assert_eq!(fixture.acknowledged().await, attempted);
        let prepared = fixture.prepared().await;
        assert!(
            prepared <= attempted + ReplayPushConfig::default().max_concurrent_document_tasks + 1,
            "{prepared} of 200 documents prepared after the stop at {raised_at}"
        );
        assert!(
            error.contains(&format!("({attempted} of 200 acknowledged)")),
            "{error}"
        );
    })
    .unwrap();
}

#[test]
fn a_stop_reaches_started_tasks_still_waiting_on_the_pacer() {
    workers::block_on(async {
        let fixture = Fixture::new(20, 0).await;
        let stop = SelectedPushStop::new();
        let raise = stop.clone();
        let (transport, received) = fixture.receiver(Some(Arc::new(move |_| {
            raise.stop("the test gave up");
            Box::pin(async { None })
        })));
        // One token now, then one every 200 ms: the other started tasks are
        // paced, not yet sent, when the first reply raises the stop.
        let error = fixture
            .push(
                &transport,
                ReplayPushConfig {
                    per_peer_rate_limit_burst: 1,
                    per_peer_rate_limit_rate: 5.0,
                    ..Default::default()
                },
                &stop,
            )
            .await
            .unwrap_err();
        assert!(error.contains("stopped: the test gave up"), "{error}");
        assert_eq!(transport.arrivals().len(), 1, "{error}");
        assert_eq!(fixture.assert_only_selected_sent(&received).len(), 1);
        assert_eq!(fixture.acknowledged().await, 1);
    })
    .unwrap();
}

/// One `DocumentPush.lean` stop row: three selected documents through one
/// send slot, the first PushLog ending with `outcome` and the rest
/// acknowledged, must send `sends` PushLogs, stop exactly when `stops` says,
/// and succeed exactly when `ok` says.
pub(super) async fn exercise_stop_case(outcome: &str, stops: bool, sends: usize, ok: bool) {
    let fixture = Fixture::new(3, 2).await;
    let first: Option<p2p::Result<PushLogReply>> = match outcome {
        "acknowledged" => None,
        "refused" => Some(Ok(PushLogReply::error(
            "selected-transfer",
            "receiver refused the head",
        ))),
        "hinted" => Some(Ok(pause_hint())),
        "disconnected" => Some(Err(p2p::Error::ConnectionClosed)),
        "timedOut" => None,
        other => panic!("unknown model outcome: {other}"),
    };
    let silent = outcome == "timedOut";
    let (transport, received) = fixture.receiver(Some(Arc::new(move |index| {
        let first = first.as_ref().map(|reply| match reply {
            Ok(reply) => Ok(reply.clone()),
            Err(_) => Err(p2p::Error::ConnectionClosed),
        });
        Box::pin(async move {
            if index > 0 {
                return None;
            }
            if silent {
                futures::future::pending::<()>().await;
            }
            first
        })
    })));
    let result = fixture
        .push(
            &transport,
            ReplayPushConfig {
                max_concurrent_document_tasks: 1,
                max_concurrent_outbound_pushes: 1,
                send_timeout: Duration::from_millis(200),
                ..Default::default()
            },
            &SelectedPushStop::new(),
        )
        .await;
    assert_eq!(transport.arrivals().len(), sends, "{outcome}: {result:?}");
    assert_eq!(result.is_ok(), ok, "{outcome}: {result:?}");
    let stopped = result
        .as_ref()
        .err()
        .is_some_and(|error| error.contains("stopped: "));
    assert_eq!(stopped, stops, "{outcome}: {result:?}");
    fixture.assert_only_selected_sent(&received);
    fixture.assert_subscription_untouched().await;
}
