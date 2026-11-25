//! The import is one admitted request of the node, run as a task of its own,
//! and each of its writes continues it.

use std::pin::pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use db::backup::{DbJob, ImportHost};
use defra_core::thread_bounds::MaybeBoxFuture;
use defra_core::RequestContext;
use futures::future::Either;
use query::{QueryRequest, QueryResponse};
use regolith::sync::Event;

use super::NodeHost;
use crate::{Node, NodeConfig};

const USERS: &str = r#"{"User": [{"name": "John"}, {"name": "Bob"}]}"#;

const ONE_USER: &str =
    r#"{"User": [{"_docID": "bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9", "name": "John"}]}"#;

const SAME_FILE_ID: &str =
    r#"{"User": [{"_docID": "bae-3fc941b7-505c-5ce2-91a0-b180930ec8a9", "name": "Bob"}]}"#;

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
    node.add_collection(
        &defra_core::RequestContext::ambient(),
        "type User { name: String }",
    )
    .await
    .unwrap();
    node
}

fn host(node: &Node) -> NodeHost {
    NodeHost::new(node, RequestContext::default()).unwrap()
}

fn requests(node: &Node) -> edge::Edge {
    node.network.as_ref().unwrap().requests().clone()
}

/// What [`Watching`] saw.
#[derive(Default)]
struct Seen {
    on_worker: AtomicBool,
    writes: AtomicUsize,
}

/// [`NodeHost`], noting whether the driver ever called it from a worker.
struct Watching {
    inner: NodeHost,
    seen: Arc<Seen>,
}

impl ImportHost for Watching {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        self.seen
            .on_worker
            .fetch_or(workers::current_worker().is_some(), Ordering::SeqCst);
        self.seen.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.run(request)
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>> {
        self.seen
            .on_worker
            .fetch_or(workers::current_worker().is_some(), Ordering::SeqCst);
        self.inner.on_db(job)
    }
}

#[test]
fn the_driver_runs_off_the_workers_and_saves_a_collection_in_one_write() {
    workers::block_on(async {
        let node = users().await;
        let seen = Arc::new(Seen::default());
        let watching = Watching {
            inner: host(&node),
            seen: Arc::clone(&seen),
        };
        let stats = node
            .import_admitted(RequestContext::default(), watching, USERS)
            .await
            .unwrap();
        assert_eq!(stats.documents_imported, 2);
        assert_eq!(seen.writes.load(Ordering::SeqCst), 1);
        assert!(
            !seen.on_worker.load(Ordering::SeqCst),
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

/// What [`PausedUntilClose`] counted once shutdown stopped admitting.
#[derive(Default)]
struct AfterClose {
    checked: Event,
    writes: AtomicUsize,
    jobs: AtomicUsize,
    failures: AtomicUsize,
}

/// [`NodeHost`], holding the import after its access check until shutdown
/// has stopped admitting requests, then counting what it runs after that.
struct PausedUntilClose {
    inner: NodeHost,
    requests: edge::Edge,
    after: Arc<AfterClose>,
}

impl PausedUntilClose {
    fn note(&self, counter: &AtomicUsize, failed: bool) {
        if self.requests.is_closed() {
            counter.fetch_add(1, Ordering::SeqCst);
            if failed {
                self.after.failures.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

impl ImportHost for PausedUntilClose {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        Box::pin(async move {
            let response = self.inner.run(request).await;
            self.note(&self.after.writes, !response.errors.is_empty());
            response
        })
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let outcome = self.inner.on_db(job).await;
            if !self.after.checked.is_set() {
                self.after.checked.set();
                self.requests
                    .timeout(Duration::from_secs(10), self.requests.closing())
                    .await
                    .expect("shutdown stops admitting requests");
                return outcome;
            }
            self.note(&self.after.jobs, outcome.is_err());
            outcome
        })
    }
}

#[test]
fn shutdown_drains_a_running_import_with_its_writes_and_aliases() {
    workers::block_on(async {
        let node = users().await;
        let after = Arc::new(AfterClose::default());
        let paused = PausedUntilClose {
            inner: host(&node),
            requests: requests(&node),
            after: Arc::clone(&after),
        };
        let finished = AtomicBool::new(false);
        let import = async {
            let outcome = node
                .import_admitted(RequestContext::default(), paused, ONE_USER)
                .await;
            finished.store(true, Ordering::SeqCst);
            outcome
        };
        let shutdown = async {
            after.checked.wait().await;
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
        assert_eq!(after.writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            after.jobs.load(Ordering::SeqCst),
            2,
            "the alias check and the alias registration run in the drain"
        );
        assert_eq!(after.failures.load(Ordering::SeqCst), 0);
    })
    .unwrap()
}

#[test]
fn an_import_after_shutdown_begins_is_refused_before_it_writes() {
    workers::block_on(async {
        let node = users().await;
        node.shutdown().await.unwrap();
        let seen = Arc::new(Seen::default());
        let watching = Watching {
            inner: host(&node),
            seen: Arc::clone(&seen),
        };
        let refused = node
            .import_admitted(RequestContext::default(), watching, ONE_USER)
            .await;
        assert!(
            matches!(
                refused,
                Err(crate::Error::EdgeRuntime(edge::EdgeError::Closed))
            ),
            "{refused:?}"
        );
        assert_eq!(seen.writes.load(Ordering::SeqCst), 0);
    })
    .unwrap()
}

/// Signals of [`Abandoned`]: the import's create has started, and a store
/// job has finished after it (only the alias registration comes after the
/// create).
#[derive(Default)]
struct AfterCreate {
    creating: Event,
    registered: Event,
}

/// [`NodeHost`], signalling the start of the create and the end of the
/// alias registration after it.
struct Abandoned {
    inner: NodeHost,
    signals: Arc<AfterCreate>,
}

impl ImportHost for Abandoned {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        self.signals.creating.set();
        self.inner.run(request)
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let outcome = self.inner.on_db(job).await;
            if self.signals.creating.is_set() {
                self.signals.registered.set();
            }
            outcome
        })
    }
}

#[test]
fn a_saved_batch_gets_its_aliases_when_the_caller_stops_waiting() {
    workers::block_on(async {
        let node = users().await;
        let signals = Arc::new(AfterCreate::default());
        let abandoned = Abandoned {
            inner: host(&node),
            signals: Arc::clone(&signals),
        };
        let import = pin!(node.import_admitted(RequestContext::default(), abandoned, ONE_USER));
        let creating = pin!(signals.creating.wait());
        let stopped = futures::future::select(import, creating).await;
        assert!(
            matches!(stopped, Either::Right(_)),
            "the caller stops waiting while the create runs"
        );
        drop(stopped);

        let requests = requests(&node);
        requests
            .timeout(Duration::from_secs(10), signals.registered.wait())
            .await
            .expect("the saved document's file ID is registered after its caller stopped waiting");
        let again = node
            .import_backup_as(&RequestContext::default(), SAME_FILE_ID)
            .await;
        assert!(
            matches!(&again, Err(error) if error.to_string().contains("a document with the given ID already exists")),
            "{again:?}"
        );
        let users = node
            .execute_request(QueryRequest::new("query { User { name } }"))
            .await;
        assert_eq!(
            users.data,
            Some(serde_json::json!({ "User": [{ "name": "John" }] }))
        );
        node.shutdown().await.unwrap();
    })
    .unwrap()
}
