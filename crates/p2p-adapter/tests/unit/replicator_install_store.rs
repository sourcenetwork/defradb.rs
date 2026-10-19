use super::*;
use events::{ChannelBus, EventName};
use storage::{backends::RegolithStore, stores::Peerstore};

pub(crate) struct Harness {
    pub(crate) store: Arc<Peerstore<RegolithStore>>,
    pub(crate) bus: Arc<ChannelBus>,
    waiters: Arc<ReplicatorInstalls>,
}

impl Default for Harness {
    fn default() -> Self {
        Self {
            store: Arc::new(Peerstore::new(Arc::new(
                RegolithStore::in_memory().unwrap(),
            ))),
            bus: Arc::default(),
            waiters: Arc::default(),
        }
    }
}

fn retry() -> RetryOptions {
    corekv::engine_retry_options(5)
}

impl Harness {
    pub(crate) async fn prepare(
        &self,
        request: &ReplicatorInstallRequest,
    ) -> P2PResult<ReplicatorInstall> {
        prepare(&*self.store, retry(), &self.waiters, request).await
    }

    pub(crate) async fn finish(
        &self,
        plan: &ReplicatorInstall,
        error: Option<String>,
    ) -> P2PResult<()> {
        self.finish_to(plan, error, Some(self.bus.clone())).await
    }

    pub(crate) async fn finish_to(
        &self,
        plan: &ReplicatorInstall,
        error: Option<String>,
        bus: Option<Arc<dyn Bus>>,
    ) -> P2PResult<()> {
        finish(
            &*self.store,
            retry(),
            self.waiters.clone(),
            bus,
            plan,
            error,
        )
        .await
    }

    async fn recover(&self) {
        recover(
            &*self.store,
            retry(),
            self.waiters.clone(),
            Some(self.bus.clone()),
        )
        .await
        .unwrap();
    }
}

fn request(peer: &str, names: &[&str]) -> ReplicatorInstallRequest {
    ReplicatorInstallRequest::new(
        p2p::ReplicatorInfo::from_raw(
            peer.to_owned(),
            names.iter().map(|name| format!("cid-{name}")).collect(),
            vec![],
        ),
        names.iter().map(|name| name.to_string()).collect(),
        &[],
    )
}

#[tokio::test]
async fn finish_is_fenced_idempotent_and_preserves_the_requested_set() {
    let h = Harness::default();
    let mut events = h.bus.subscribe(&[EventName::ReplicatorCompleted]);
    let first = h.prepare(&request("peer", &["Note"])).await.unwrap();
    h.finish(&first, None).await.unwrap();
    assert_eq!(
        events
            .recv()
            .await
            .unwrap()
            .as_replicator_completed()
            .unwrap()
            .collections,
        ["Note"]
    );
    let second = h
        .prepare(&request("peer", &["Note", "User"]))
        .await
        .unwrap();
    assert_eq!(second.replay, ["User"]);
    h.finish(&first, None).await.unwrap();
    assert!(events.try_recv().is_err());
    h.finish(&second, None).await.unwrap();
    let message = events.recv().await.unwrap();
    let data = message.as_replicator_completed().unwrap();
    assert_eq!(data.collections, ["Note", "User"]);
    assert!(!data.skipped);
    h.finish(&second, Some("duplicate".into())).await.unwrap();
    assert!(events.try_recv().is_err());
    let third = h
        .prepare(&request("peer", &["Note", "User"]))
        .await
        .unwrap();
    assert!(third.replay.is_empty());
    h.finish(&third, None).await.unwrap();
    assert!(
        events
            .recv()
            .await
            .unwrap()
            .as_replicator_completed()
            .unwrap()
            .skipped
    );
}

#[tokio::test]
async fn waiting_has_a_deadline_and_does_not_clear_the_active_install() {
    let h = Harness::default();
    let request = request("peer", &["Note"]);
    let first = h.prepare(&request).await.unwrap();
    let error = prepare_with_timeout(
        &*h.store,
        retry(),
        &h.waiters,
        &request,
        Duration::from_millis(5),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("still active"));
    let saved: ReplicatorInstall = serde_json::from_slice(
        &h.store
            .new_txn(true)
            .await
            .unwrap()
            .get(&key("peer"))
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(saved.pending);
    assert_eq!(saved.id, first.id);
    assert!(h.waiters.is_empty());
    let other = h
        .prepare(&super::tests::request("other", &["Note"]))
        .await
        .unwrap();
    h.finish(&other, None).await.unwrap();
    h.finish(&first, None).await.unwrap();
    let next = h.prepare(&request).await.unwrap();
    assert!(next.replay.is_empty());
}

#[tokio::test]
async fn abandoned_replay_reports_interruption_once_and_forces_a_new_replay() {
    let h = Harness::default();
    let mut events = h.bus.subscribe(&[EventName::ReplicatorCompleted]);
    let request = request("peer", &["Note"]);
    let mut first = h.prepare(&request).await.unwrap();
    h.recover().await;
    assert!(
        events.try_recv().is_err(),
        "recovery cannot finish this process's active replay"
    );
    first._owner.take();
    h.recover().await;
    let message = events.recv().await.unwrap();
    let data = message.as_replicator_completed().unwrap();
    assert_eq!(data.error.as_deref(), Some(INTERRUPTED));
    assert_eq!(data.collections, ["Note"]);
    assert!(!data.skipped);
    h.recover().await;
    h.finish(&first, None).await.unwrap();
    assert!(events.try_recv().is_err());
    let next = h.prepare(&request).await.unwrap();
    assert_eq!(next.replay, ["Note"]);
}

#[tokio::test]
async fn changed_and_removed_capabilities_trigger_replay_without_persisting_tokens() {
    let h = Harness::default();
    let mut request = request("peer", &["Note"]);
    request
        .capabilities
        .insert("cid-Note".into(), Sha256::digest(b"capability-one").into());
    let first = h.prepare(&request).await.unwrap();
    let bytes = h
        .store
        .new_txn(true)
        .await
        .unwrap()
        .get(&key("peer"))
        .await
        .unwrap()
        .unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("capability-one"));
    h.finish(&first, None).await.unwrap();
    let skipped = h.prepare(&request).await.unwrap();
    assert!(skipped.replay.is_empty());
    h.finish(&skipped, None).await.unwrap();
    request.capabilities.clear();
    let changed = h.prepare(&request).await.unwrap();
    assert_eq!(changed.replay, ["Note"]);
}

#[tokio::test]
async fn malformed_durable_state_fails_without_overwriting_it() {
    let h = Harness::default();
    h.store
        .create_replicator("peer", b"not cbor")
        .await
        .unwrap();
    assert!(h.prepare(&request("peer", &["Note"])).await.is_err());
    assert_eq!(
        h.store
            .get_replicator("peer")
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        b"not cbor"
    );
}

#[tokio::test]
async fn a_replaced_peer_configuration_cannot_report_the_old_replay_as_successful() {
    let h = Harness::default();
    let mut events = h.bus.subscribe(&[EventName::ReplicatorCompleted]);
    let first = h.prepare(&request("peer", &["Note"])).await.unwrap();
    let replacement = request("peer", &["User"]);
    h.store
        .create_replicator("peer", &replacement.info.to_bytes().unwrap())
        .await
        .unwrap();
    h.finish(&first, None).await.unwrap();
    let message = events.recv().await.unwrap();
    assert!(message
        .as_replicator_completed()
        .unwrap()
        .error
        .as_deref()
        .unwrap()
        .contains("configuration changed"));
}

struct PauseFirstAttempt {
    inner: Prepare,
    first: bool,
    staged: Arc<regolith::sync::Event>,
    resume: Arc<regolith::sync::Event>,
}

struct CorruptInstall;

impl RetryAttemptFn for CorruptInstall {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            txn.set(&key("broken"), b"invalid install").await?;
            Ok(Box::new(()) as RetryAttemptValue)
        })
    }
}

#[tokio::test]
async fn one_corrupt_record_does_not_block_recovery_of_other_peers() {
    let h = Harness::default();
    let mut events = h.bus.subscribe(&[EventName::ReplicatorCompleted]);
    let mut broken = h.prepare(&request("broken", &["Note"])).await.unwrap();
    broken._owner.take();
    corekv::transact::<_, ()>(&*h.store, retry(), None, Box::new(CorruptInstall))
        .await
        .unwrap();
    let mut healthy = h.prepare(&request("healthy", &["Note"])).await.unwrap();
    healthy._owner.take();
    h.recover().await;
    let message = events.recv().await.unwrap();
    let data = message.as_replicator_completed().unwrap();
    assert_eq!(data.peer_id, "healthy");
    assert_eq!(data.error.as_deref(), Some(INTERRUPTED));
    assert!(events.try_recv().is_err());
}

impl RetryAttemptFn for PauseFirstAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let result = self.inner.run(txn).await?;
            if std::mem::take(&mut self.first) {
                self.staged.set();
                self.resume.wait().await;
            }
            Ok(result)
        })
    }
}

#[tokio::test]
async fn conflicting_installs_redecide_in_the_engine_transaction() {
    let h = Harness::default();
    let id = h
        .store
        .allocate(b"/rep/incarnation/sequence", 1)
        .await
        .unwrap()
        .end;
    let staged = Arc::new(regolith::sync::Event::new());
    let resume = Arc::new(regolith::sync::Event::new());
    let attempt = PauseFirstAttempt {
        inner: Prepare {
            request: request("peer", &["Note"]),
            id,
            owner: h.waiters.runtime.own_install("peer", id).unwrap(),
        },
        first: true,
        staged: staged.clone(),
        resume: resume.clone(),
    };
    let store = h.store.clone();
    let loser = tokio::spawn(async move {
        corekv::transact::<_, Decision>(&*store, retry(), None, Box::new(attempt))
            .await
            .unwrap()
    });
    staged.wait().await;
    let winner = h.prepare(&request("peer", &["User"])).await.unwrap();
    resume.set();
    assert!(matches!(loser.await.unwrap(), Decision::Busy(id) if id == winner.id));
    let row = h.store.get_replicator("peer").await.unwrap().unwrap();
    assert_eq!(
        p2p::ReplicatorInfo::from_bytes(&row).unwrap().collections,
        ["cid-User"]
    );
    h.finish(&winner, None).await.unwrap();
    let next = h.prepare(&request("peer", &["Note"])).await.unwrap();
    assert_eq!(next.replay, ["Note"]);
}
