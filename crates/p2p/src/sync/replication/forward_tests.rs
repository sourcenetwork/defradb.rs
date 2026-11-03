//! A hub that merges an upstream head owes it to its own replicators. The
//! upstream obligation (the durable pending record) may be discharged only
//! once the downstream one (a durable replicator marker) exists.

use super::super::handlers::process_one_event;
use super::*;
use crate::sync::coordinator::SyncCoordinator;
use crate::sync::pending_store::{PendingDagStorage, PendingDagStore, PersistedPendingDag};

type TestCoordinator = SyncCoordinator<DefraBlockstore<RegolithStore>, NoopTransport>;

const DOWNSTREAM: &str = "downstream-peer";

/// The hub's durable marker recorder. While `failing` it refuses every
/// registration, as a peerstore error or a closed recorder does.
#[derive(Clone, Default)]
struct Recorder {
    failing: Arc<AtomicBool>,
    durable: Arc<AtomicUsize>,
}

impl Recorder {
    fn install(&self, coordinator: &mut TestCoordinator) {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<crate::sync::PushFailure>(8);
        coordinator.set_failure_channel(tx);
        let recorder = self.clone();
        n0_future::task::spawn(async move {
            while let Some(mut event) = rx.recv().await {
                let Some(durable_tx) = event.durable_tx.take() else {
                    continue;
                };
                if recorder.failing.load(Ordering::SeqCst) {
                    let _ = durable_tx.send(None);
                    continue;
                }
                if event.peer_id == DOWNSTREAM && !event.acknowledged {
                    recorder.durable.fetch_add(1, Ordering::SeqCst);
                }
                let _ = durable_tx.send(Some(event.marker_generation.unwrap_or(1)));
            }
        });
    }

    fn markers(&self) -> usize {
        self.durable.load(Ordering::SeqCst)
    }
}

/// The database's answers for one root: merged the first time, then the
/// "already merged" skip, in this process or after a restart, because the
/// database answers from its durable head publication.
struct HubMergeHandler {
    applied: Arc<AtomicBool>,
}

#[async_trait]
impl MergeHandler for HubMergeHandler {
    type Error = TestError;

    async fn handle_block(
        &self,
        _cid: &Cid,
        _block_data: &[u8],
        _metadata: BlockMetadata<'_>,
    ) -> Result<MergeOutcome, Self::Error> {
        if self.applied.swap(true, Ordering::SeqCst) {
            Ok(MergeOutcome::terminal_skip(MergeOutcome::ALREADY_MERGED))
        } else {
            Ok(MergeOutcome::Merged)
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Path {
    Single,
    Batch,
}

struct Hub {
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    pending: Arc<PendingDagStore<RegolithStore>>,
    applied: Arc<AtomicBool>,
    root: Cid,
}

impl Hub {
    async fn new() -> Self {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let blockstore = Arc::new(DefraBlockstore::new(store.clone(), true));
        let pending = Arc::new(PendingDagStore::new(store));
        let block = b"hub-forward-head";
        let root = make_cid(block);
        blockstore.put(&root, block).await.unwrap();
        pending
            .put(
                &root,
                &PersistedPendingDag {
                    doc_id: "doc1".to_string(),
                    collection_id: "col1".to_string(),
                    head_priority: None,
                    creator: "did:key:creator".to_string(),
                    source_peer: Some("upstream-peer".to_string()),
                    alternate_providers: Vec::new(),
                    is_explicit_replicator: true,
                    explicit_replay_authorization: None,
                },
            )
            .await
            .unwrap();
        Self {
            blockstore,
            pending,
            applied: Arc::new(AtomicBool::new(false)),
            root,
        }
    }

    /// A merge handler over the hub's database, as one process builds it.
    fn handler(&self) -> HubMergeHandler {
        HubMergeHandler {
            applied: self.applied.clone(),
        }
    }

    async fn start(&self, recorder: &Recorder) -> TestCoordinator {
        let transport = NoopTransport::new();
        transport.set_replicators(vec![ReplicatorInfo::from_raw(
            DOWNSTREAM.to_string(),
            vec!["col1".to_string()],
            Vec::new(),
        )]);
        let (mut coordinator, _events) = SyncCoordinator::with_access_control(
            transport,
            self.blockstore.clone(),
            crate::sync::SyncConfig::default(),
            AccessMode::Open,
            Arc::new(crate::ReplicatorRegistry::new()),
            Arc::new(crate::sync::collection_store::MemoryCollectionStorage::default()),
            Arc::new(EqOnlyFilterMatcher),
        )
        .await
        .unwrap();
        recorder.install(&mut coordinator);
        coordinator
            .install_pending_dag_store(self.pending.clone())
            .await;
        coordinator
    }

    fn dag_ready(&self) -> SyncEvent {
        SyncEvent::DagReady {
            root_cid: self.root,
            doc_id: "doc1".to_string(),
            collection_id: "col1".to_string(),
            creator: "did:key:creator".to_string(),
            sender_peer: Some("upstream-peer".to_string()),
            is_explicit_replicator: true,
            explicit_replay_authorization: None,
        }
    }

    async fn drive(
        &self,
        coordinator: &TestCoordinator,
        handler: &HubMergeHandler,
        path: Path,
    ) -> ReplicationResult {
        let config = ReplicationConfig::default();
        match path {
            Path::Single => {
                process_one_event(coordinator, self.dag_ready(), handler, &config).await
            }
            Path::Batch => {
                process_merge_batch(coordinator, vec![self.dag_ready()], handler, &config)
                    .await
                    .pop()
                    .expect("one result per event")
            }
        }
    }

    async fn upstream_owned(&self) -> bool {
        self.pending
            .load_all()
            .await
            .unwrap()
            .iter()
            .any(|(cid, _)| *cid == self.root)
    }

    async fn assert_obligation_owned(&self, recorder: &Recorder, when: &str) {
        assert!(
            recorder.markers() > 0 || self.upstream_owned().await,
            "{when}: the hub dropped its upstream record for {} while no downstream marker was durable",
            self.root
        );
    }
}

#[tokio::test]
async fn failed_hub_forward_keeps_the_upstream_obligation_until_a_redrive_forwards() {
    for path in [Path::Single, Path::Batch] {
        let hub = Hub::new().await;
        let recorder = Recorder::default();
        let coordinator = hub.start(&recorder).await;
        let handler = hub.handler();

        recorder.failing.store(true, Ordering::SeqCst);
        let first = hub.drive(&coordinator, &handler, path).await;
        assert!(
            matches!(first, ReplicationResult::MergedButBroadcastFailed { .. }),
            "{path:?}: {first:?}"
        );
        hub.assert_obligation_owned(&recorder, &format!("{path:?} after the failed forward"))
            .await;

        recorder.failing.store(false, Ordering::SeqCst);
        hub.drive(&coordinator, &handler, path).await;
        assert!(
            recorder.markers() > 0,
            "{path:?}: the re-driven root was discharged without a downstream marker"
        );
        assert!(
            !hub.upstream_owned().await,
            "{path:?}: the re-drive did not discharge"
        );
        assert!(hub.blockstore.is_merged(&hub.root).await.unwrap());
    }
}

#[tokio::test]
async fn hub_forward_cut_by_shutdown_is_forwarded_after_restart() {
    for path in [Path::Single, Path::Batch] {
        let hub = Hub::new().await;
        let recorder = Recorder::default();
        let coordinator = hub.start(&recorder).await;
        coordinator.shutdown().await;
        let first = hub.drive(&coordinator, &hub.handler(), path).await;
        assert!(
            matches!(first, ReplicationResult::MergedButBroadcastFailed { .. }),
            "{path:?}: {first:?}"
        );
        hub.assert_obligation_owned(&recorder, &format!("{path:?} after shutdown"))
            .await;
        drop(coordinator);

        let restarted = hub.start(&recorder).await;
        hub.drive(&restarted, &hub.handler(), path).await;
        assert!(
            recorder.markers() > 0,
            "{path:?}: the restarted hub discharged the root without a downstream marker"
        );
        assert!(
            !hub.upstream_owned().await,
            "{path:?}: the restart did not discharge"
        );
    }
}
