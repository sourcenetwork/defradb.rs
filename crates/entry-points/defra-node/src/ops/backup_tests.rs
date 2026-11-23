//! An import's driver runs on its caller, the import is one admitted
//! request of the node, and each of its writes continues it.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use db::backup::{DbJob, ImportHost};
use defra_core::thread_bounds::MaybeBoxFuture;
use defra_core::RequestContext;
use query::{QueryRequest, QueryResponse};
use regolith::sync::Event;

use super::NodeHost;
use crate::{Node, NodeConfig};

const USERS: &str = r#"{"User": [{"name": "John"}, {"name": "Bob"}]}"#;

const ONE_USER: &str =
    r#"{"User": [{"_docID": "bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9", "name": "John"}]}"#;

/// A node with a network edge, so its requests are admitted, and a `User`
/// collection.
async fn users() -> Node {
    let node = Node::builder()
        .with_config(NodeConfig {
            edge: Some(edge::testing::edge()),
            ..Default::default()
        })
        .build()
        .await
        .unwrap();
    node.add_collection("type User { name: String }")
        .await
        .unwrap();
    node
}

fn host(node: &Node) -> NodeHost<'_> {
    NodeHost {
        node,
        context: RequestContext::default(),
        identity: None,
    }
}

/// [`NodeHost`], noting whether the driver ever called it from a worker.
struct Watching<'a> {
    inner: NodeHost<'a>,
    on_worker: AtomicBool,
    writes: AtomicUsize,
}

impl ImportHost for Watching<'_> {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        self.on_worker
            .fetch_or(workers::current_worker().is_some(), Ordering::SeqCst);
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.run(request)
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>> {
        self.on_worker
            .fetch_or(workers::current_worker().is_some(), Ordering::SeqCst);
        self.inner.on_db(job)
    }
}

#[test]
fn the_driver_runs_on_its_caller_and_saves_a_collection_in_one_write() {
    workers::block_on(async {
        let node = users().await;
        let watching = Watching {
            inner: host(&node),
            on_worker: AtomicBool::new(false),
            writes: AtomicUsize::new(0),
        };
        let stats = node
            .ops()
            .import_backup_with(RequestContext::default(), &watching, USERS)
            .await
            .unwrap();
        assert_eq!(stats.documents_imported, 2);
        assert_eq!(watching.writes.load(Ordering::SeqCst), 1);
        assert!(
            !watching.on_worker.load(Ordering::SeqCst),
            "the driver never runs on a database worker"
        );
        node.shutdown().await.unwrap();
    })
    .unwrap()
}

#[test]
fn a_write_after_shutdown_begins_is_refused() {
    workers::block_on(async {
        let node = users().await;
        let host = host(&node);
        let write = || QueryRequest::new(r#"mutation { add_User(input: {name: "x"}) { _docID } }"#);
        let before = host.run(write()).await;
        assert!(before.errors.is_empty(), "{:?}", before.errors);

        node.shutdown().await.unwrap();
        let after = host.run(write()).await;
        assert_eq!(
            after.errors.first().map(|error| error.message.as_str()),
            Some("the node is shutting down")
        );
    })
    .unwrap()
}

/// [`NodeHost`], holding the import after its access check until shutdown
/// has stopped admitting requests, then counting what it runs after that.
struct PausedUntilClose<'a> {
    inner: NodeHost<'a>,
    requests: edge::Edge,
    checked: Event,
    writes_after_close: AtomicUsize,
    jobs_after_close: AtomicUsize,
    failures_after_close: AtomicUsize,
}

impl PausedUntilClose<'_> {
    fn note(&self, counter: &AtomicUsize, failed: bool) {
        if self.requests.is_closed() {
            counter.fetch_add(1, Ordering::SeqCst);
            if failed {
                self.failures_after_close.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

impl ImportHost for PausedUntilClose<'_> {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        Box::pin(async move {
            let response = self.inner.run(request).await;
            self.note(&self.writes_after_close, !response.errors.is_empty());
            response
        })
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let outcome = self.inner.on_db(job).await;
            if !self.checked.is_set() {
                self.checked.set();
                self.requests
                    .timeout(Duration::from_secs(10), self.requests.closing())
                    .await
                    .expect("shutdown stops admitting requests");
                return outcome;
            }
            self.note(&self.jobs_after_close, outcome.is_err());
            outcome
        })
    }
}

#[test]
fn shutdown_drains_a_running_import_with_its_writes_and_aliases() {
    workers::block_on(async {
        let node = users().await;
        let paused = PausedUntilClose {
            inner: host(&node),
            requests: node.network.as_ref().unwrap().requests().clone(),
            checked: Event::new(),
            writes_after_close: AtomicUsize::new(0),
            jobs_after_close: AtomicUsize::new(0),
            failures_after_close: AtomicUsize::new(0),
        };
        let finished = AtomicBool::new(false);
        let import = async {
            let outcome = node
                .import_admitted(RequestContext::default(), &paused, ONE_USER)
                .await;
            finished.store(true, Ordering::SeqCst);
            outcome
        };
        let shutdown = async {
            paused.checked.wait().await;
            let outcome = node.shutdown().await;
            (outcome, finished.load(Ordering::SeqCst))
        };
        let (imported, (shut, import_ended_first)) = futures::future::join(import, shutdown).await;

        assert_eq!(imported.unwrap().documents_imported, 1);
        shut.unwrap();
        assert!(
            import_ended_first,
            "shutdown finished before the import it admitted ended"
        );
        assert_eq!(paused.writes_after_close.load(Ordering::SeqCst), 1);
        assert_eq!(
            paused.jobs_after_close.load(Ordering::SeqCst),
            2,
            "the alias check and the alias registration run in the drain"
        );
        assert_eq!(paused.failures_after_close.load(Ordering::SeqCst), 0);
    })
    .unwrap()
}

#[test]
fn an_import_after_shutdown_begins_is_refused_before_it_writes() {
    workers::block_on(async {
        let node = users().await;
        node.shutdown().await.unwrap();
        let watching = Watching {
            inner: host(&node),
            on_worker: AtomicBool::new(false),
            writes: AtomicUsize::new(0),
        };
        let refused = node
            .import_admitted(RequestContext::default(), &watching, ONE_USER)
            .await;
        assert!(
            matches!(
                refused,
                Err(crate::Error::EdgeRuntime(edge::EdgeError::Closed))
            ),
            "{refused:?}"
        );
        assert_eq!(watching.writes.load(Ordering::SeqCst), 0);
    })
    .unwrap()
}
