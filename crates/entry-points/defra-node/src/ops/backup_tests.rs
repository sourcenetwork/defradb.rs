//! An import's driver runs on its caller, and each of its writes is an
//! admitted request of the node.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use db::backup::{DbJob, ImportHost};
use defra_core::thread_bounds::MaybeBoxFuture;
use defra_core::RequestContext;
use query::{QueryRequest, QueryResponse};

use super::NodeHost;
use crate::{Node, NodeConfig};

const USERS: &str = r#"{"User": [{"name": "John"}, {"name": "Bob"}]}"#;

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
