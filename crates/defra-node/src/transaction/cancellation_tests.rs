use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use query::{QueryExecutor, QueryRequest, QueryResponse, TransactionError, TransactionHandle};

#[derive(Default)]
struct HeldCommit {
    started: regolith::sync::Event,
    release: regolith::sync::Event,
    completed: regolith::sync::Event,
    abandoned: AtomicBool,
    abandoned_event: regolith::sync::Event,
    hold_begin: bool,
}

#[async_trait::async_trait]
impl QueryExecutor for HeldCommit {
    fn abandon_txn(&self, _handle: &TransactionHandle) {
        self.abandoned.store(true, Ordering::SeqCst);
        self.abandoned_event.set();
    }

    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        QueryResponse::success(serde_json::json!({}))
    }

    async fn execute_in_txn(
        &self,
        request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.execute(request).await
    }

    async fn begin_txn(&self, _readonly: bool) -> Result<TransactionHandle, TransactionError> {
        if self.hold_begin {
            self.started.set();
            self.release.wait().await;
            self.completed.set();
        }
        Ok(TransactionHandle::new("42".to_string()))
    }

    async fn commit_txn(&self, _handle: &TransactionHandle) -> Result<(), TransactionError> {
        self.started.set();
        self.release.wait().await;
        self.completed.set();
        Ok(())
    }

    async fn discard_txn(&self, _handle: &TransactionHandle) -> Result<(), TransactionError> {
        Ok(())
    }

    async fn schema(&self) -> query::Result<String> {
        Ok(String::new())
    }
}

#[test]
fn cancelling_a_commit_waiter_does_not_abandon_the_running_commit() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let mut node = super::Node::builder()
            .with_edge(edge.clone())
            .build()
            .await
            .unwrap();
        let executor = Arc::new(HeldCommit::default());
        node.runner = executor.clone();
        let txn = node.begin_transaction_guard(false).await.unwrap();
        {
            let commit = txn.commit();
            let mut commit = std::pin::pin!(commit);
            futures::future::poll_fn(|cx| {
                let result = commit.as_mut().poll(cx);
                assert!(result.is_pending());
                if executor.started.is_set() {
                    std::task::Poll::Ready(())
                } else {
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
            })
            .await;
        }
        let abandoned_while_committing = executor.abandoned.load(Ordering::SeqCst);
        executor.release.set();
        edge.timeout(Duration::from_secs(5), executor.completed.wait())
            .await
            .unwrap();
        node.shutdown().await.unwrap();
        assert!(
            !abandoned_while_committing,
            "the guard abandoned a commit still running"
        );
    })
    .unwrap();
}

#[test]
fn cancelling_a_begin_waiter_abandons_the_unreturned_transaction() {
    let edge = edge::testing::edge();
    edge.block_on(async {
        let mut node = super::Node::builder()
            .with_edge(edge.clone())
            .build()
            .await
            .unwrap();
        let executor = Arc::new(HeldCommit {
            hold_begin: true,
            ..HeldCommit::default()
        });
        node.runner = executor.clone();
        {
            let begin = node.begin_transaction_guard(false);
            let mut begin = std::pin::pin!(begin);
            futures::future::poll_fn(|cx| {
                let result = begin.as_mut().poll(cx);
                assert!(result.is_pending());
                if executor.started.is_set() {
                    std::task::Poll::Ready(())
                } else {
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
            })
            .await;
        }
        executor.release.set();
        edge.timeout(Duration::from_secs(5), executor.completed.wait())
            .await
            .unwrap();
        let abandoned = edge
            .timeout(Duration::from_secs(2), executor.abandoned_event.wait())
            .await
            .is_ok();
        node.shutdown().await.unwrap();
        assert!(abandoned, "the unreturned transaction was left open");
    })
    .unwrap();
}
