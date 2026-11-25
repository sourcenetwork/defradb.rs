use std::future::Future as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Waker};
use std::time::Duration;

use prolly::{tree::DEFAULT_TARGET, Item, MemStore, Node, Store as _, Tree};

use super::tests::{entries, entry, node_keys, publish, sweep, Publish, StoredRoots};
use super::{node_key, reclaim_every, ProllyNodes};
use crate::backends::RegolithStore;
use crate::corekv::{
    engine_retry_options, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryTxn,
};

fn leaf() -> Node {
    Node {
        level: 0,
        items: vec![Item {
            key: b"k".to_vec(),
            value: b"v".to_vec(),
        }],
    }
}

async fn stored(store: &RegolithStore, node: &Node) -> Option<Vec<u8>> {
    let txn = crate::corekv::Store::new_txn(store, true).await.unwrap();
    let stored = txn.get(&node_key(&node.hash())).await.unwrap();
    txn.discard();
    stored.map(|bytes| bytes.to_vec())
}

#[test]
fn a_put_is_readable_before_and_after_its_publish() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        let node = leaf();
        let hash = regolith.put(node.clone()).await.unwrap();
        assert!(regolith.pending_bytes() > 0);
        assert_eq!(regolith.get(&hash).await.unwrap().as_deref(), Some(&node));
        assert_eq!(
            stored(&store, &node).await,
            None,
            "a put alone writes nothing"
        );

        publish(&mut regolith, &[hash]).await;
        assert_eq!(regolith.pending_bytes(), 0);
        assert_eq!(regolith.get(&hash).await.unwrap().as_deref(), Some(&node));
        assert_eq!(stored(&store, &node).await, Some(node.encode()));
    })
    .unwrap()
}

#[test]
fn an_edit_stores_nothing_until_the_transaction_that_publishes_its_root() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        let mut tree = Tree::build(&mut regolith, DEFAULT_TARGET, (0..5_000).map(entry))
            .await
            .unwrap();
        assert!(node_keys(&store).await.is_empty(), "a build stores nothing");
        publish(&mut regolith, &[tree.root().unwrap()]).await;
        let built = node_keys(&store).await.len();

        tree.remove(&mut regolith, &entry(2_499).0).await.unwrap();
        assert_eq!(
            node_keys(&store).await.len(),
            built,
            "the edit's nodes wait"
        );
        // A page between the edit and its publication sees only the old tree,
        // all of it live: before the edit's nodes waited for their root, they
        // sat here unnamed, and the page deleted them.
        let report = sweep(&store).await;
        assert_eq!(report.deleted, 0, "{report:?}");

        publish(&mut regolith, &[tree.root().unwrap()]).await;
        let fresh = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        assert_eq!(entries(&tree, &fresh).await.len(), 4_999);
        let report = sweep(&store).await;
        assert!(report.deleted > 0, "the old tree's own nodes go");
        assert_eq!(entries(&tree, &fresh).await.len(), 4_999);
    })
    .unwrap()
}

/// A root write that refuses to publish.
struct Refuse;

impl RetryAttemptFn for Refuse {
    fn run<'a>(&'a mut self, _txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            Err(RetryAttemptError::Storage(crate::corekv::Error::Other(
                "the root write refused".into(),
            )))
        })
    }
}

#[test]
fn a_refused_root_write_stores_none_of_its_nodes() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let mut regolith = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
        let node = leaf();
        let hash = regolith.put(node.clone()).await.unwrap();
        let pending = regolith.pending_bytes();

        let error = regolith
            .publish::<()>(Box::new(Refuse))
            .await
            .unwrap_err()
            .into_storage();
        assert!(error.to_string().contains("refused"), "{error}");
        assert_eq!(
            stored(&store, &node).await,
            None,
            "the nodes went with the root"
        );
        assert_eq!(regolith.pending_bytes(), pending);

        publish(&mut regolith, &[hash]).await;
        assert_eq!(stored(&store, &node).await, Some(node.encode()));
    })
    .unwrap()
}

/// A regolith store whose first `transact_retry` fails with an I/O error, or,
/// with `stall`, waits forever.
struct FailFirstWrite {
    inner: Arc<RegolithStore>,
    failed: AtomicBool,
    stall: bool,
}

impl crate::corekv::private::Sealed for FailFirstWrite {}

#[async_trait::async_trait]
impl crate::corekv::Store for FailFirstWrite {
    fn crdts(&self) -> &crdt::Crdts {
        self.inner.crdts()
    }

    async fn allocate(
        &self,
        key: &[u8],
        count: u64,
    ) -> crate::corekv::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> crate::corekv::Result<Box<dyn crate::corekv::Txn>> {
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: crate::corekv::RetryOptions,
        attempt: crate::corekv::RetryAttempt,
    ) -> Result<crate::corekv::RetryAttemptValue, RetryAttemptError> {
        if !self.failed.swap(true, Ordering::SeqCst) {
            if self.stall {
                std::future::pending::<()>().await;
            }
            return Err(RetryAttemptError::Storage(crate::corekv::Error::Other(
                "injected write failure".into(),
            )));
        }
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> crate::corekv::Result<()> {
        self.inner.close().await
    }
}

fn failing(regolith: &Arc<RegolithStore>, stall: bool) -> ProllyNodes<FailFirstWrite> {
    let store = Arc::new(FailFirstWrite {
        inner: Arc::clone(regolith),
        failed: AtomicBool::new(false),
        stall,
    });
    ProllyNodes::new(store, engine_retry_options(5))
}

fn root_write(root: &Node) -> Box<Publish> {
    Box::new(Publish {
        roots: root.hash().0.to_vec(),
    })
}

#[test]
fn a_failed_publish_keeps_its_nodes_pending_and_the_next_publish_writes_them() {
    workers::block_on(async {
        let regolith = Arc::new(RegolithStore::in_memory().unwrap());
        let mut nodes = failing(&regolith, false);
        let node = leaf();
        let hash = nodes.put(node.clone()).await.unwrap();
        let pending = nodes.pending_bytes();
        assert!(pending > 0);

        let error = nodes
            .publish::<()>(root_write(&node))
            .await
            .unwrap_err()
            .into_storage();
        assert!(
            error.to_string().contains("injected write failure"),
            "{error}"
        );
        assert_eq!(nodes.pending_bytes(), pending, "nothing was written");
        assert_eq!(nodes.get(&hash).await.unwrap().as_deref(), Some(&node));

        nodes.publish::<()>(root_write(&node)).await.unwrap();
        assert_eq!(nodes.pending_bytes(), 0);
        assert_eq!(stored(&regolith, &node).await, Some(node.encode()));
    })
    .unwrap()
}

#[test]
fn a_publish_dropped_before_its_commit_keeps_its_nodes_pending() {
    workers::block_on(async {
        let regolith = Arc::new(RegolithStore::in_memory().unwrap());
        let mut nodes = failing(&regolith, true);
        let node = leaf();
        let hash = nodes.put(node.clone()).await.unwrap();
        let pending = nodes.pending_bytes();

        {
            let mut publishing = std::pin::pin!(nodes.publish::<()>(root_write(&node)));
            let polled = publishing
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
            assert!(polled.is_pending(), "the commit waits");
        }
        assert_eq!(nodes.pending_bytes(), pending, "nothing was written");
        assert_eq!(
            nodes.get(&hash).await.unwrap().as_deref(),
            Some(&node),
            "a dropped publish loses no put"
        );

        nodes.publish::<()>(root_write(&node)).await.unwrap();
        assert_eq!(nodes.pending_bytes(), 0);
        assert_eq!(stored(&regolith, &node).await, Some(node.encode()));
    })
    .unwrap()
}

#[test]
fn scheduled_pages_reclaim_every_node_the_live_root_left_behind() {
    let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
    let (left, reachable, live) = pool
        .submit_and_wait(workers::Target::Worker(0), || async {
            let store = Arc::new(RegolithStore::in_memory().unwrap());
            let mut regolith = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
            let mut tree = Tree::build(&mut regolith, DEFAULT_TARGET, (0..5_000).map(entry))
                .await
                .unwrap();
            publish(&mut regolith, &[tree.root().unwrap()]).await;
            let removed = |n: &u32| n.is_multiple_of(500);
            for n in (0..5_000).filter(removed) {
                tree.remove(&mut regolith, &entry(n).0).await.unwrap();
                publish(&mut regolith, &[tree.root().unwrap()]).await;
            }
            // The same entries built afresh give the same nodes, so their
            // count is what the live root reaches.
            let mut memory = MemStore::new();
            let rebuilt = Tree::build(
                &mut memory,
                DEFAULT_TARGET,
                (0..5_000).filter(|n| !removed(n)).map(entry),
            )
            .await
            .unwrap();
            assert_eq!(rebuilt, tree);
            let before = node_keys(&store).await.len();
            assert!(before > memory.len(), "the edits left old nodes");

            let schedule = workers::spawn_background(None, "prolly-reclaim", {
                let store = Arc::clone(&store);
                let page = before / 4;
                move || async move {
                    let _ =
                        reclaim_every(store, Arc::new(StoredRoots), Duration::from_millis(1), page)
                            .await;
                }
            });
            let mut left = before;
            for _ in 0..2_000 {
                workers::sleep(Duration::from_millis(5)).unwrap().await;
                left = node_keys(&store).await.len();
                if left == memory.len() {
                    break;
                }
            }
            schedule.abort();
            let fresh = ProllyNodes::new(Arc::clone(&store), engine_retry_options(5));
            (left, memory.len(), entries(&tree, &fresh).await.len())
        })
        .unwrap();
    pool.shutdown_blocking().unwrap();
    assert_eq!(left, reachable, "only the live root's nodes are left");
    assert_eq!(live, 4_990, "the live tree is whole");
}
