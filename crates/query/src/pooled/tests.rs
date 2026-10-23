//! Requests run on the pool, one task per document at a time, an explicit
//! transaction's requests go to its registry, and a worker awaiting a
//! request it submitted itself never waits on itself.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use kovan_queue::seg_queue::SegQueue;

use super::PooledExecutor;
use crate::error::{Result, TransactionError};
use crate::executor::{QueryExecutor, QueryRequest, QueryResponse};
use crate::txn::TransactionHandle;

const DEADLINE: Duration = Duration::from_secs(60);

/// Records where each call ran and how many ran at once.
#[derive(Default)]
struct Recorder {
    workers: SegQueue<Option<usize>>,
    running: AtomicUsize,
    most: AtomicUsize,
}

impl Recorder {
    async fn call(&self) {
        self.workers
            .push(workers::current_worker().map(|worker| worker.index));
        let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        // Suspends a few times, so a second task for the same document would
        // run meanwhile if routing let it.
        for _ in 0..8 {
            workers::yield_now().await;
        }
        self.running.fetch_sub(1, Ordering::SeqCst);
    }

    fn workers(&self) -> Vec<Option<usize>> {
        std::iter::from_fn(|| self.workers.pop()).collect()
    }
}

#[async_trait]
impl QueryExecutor for Recorder {
    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        self.call().await;
        QueryResponse::success(serde_json::json!({}))
    }

    async fn execute_in_txn(
        &self,
        _request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.call().await;
        QueryResponse::success(serde_json::json!({}))
    }

    async fn begin_txn(
        &self,
        _readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        self.call().await;
        Ok(TransactionHandle::new("t-1".into()))
    }

    async fn commit_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        self.call().await;
        Ok(())
    }

    async fn discard_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        self.call().await;
        Ok(())
    }

    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn schema(&self) -> Result<String> {
        self.call().await;
        Ok(String::new())
    }
}

fn pooled(workers: usize) -> (Arc<Recorder>, Arc<PooledExecutor>, workers::Pool) {
    let pool = workers::Pool::start(workers::Config::unplaced(workers)).unwrap();
    let recorder = Arc::new(Recorder::default());
    let inner: Arc<dyn QueryExecutor> = recorder.clone();
    (
        recorder,
        Arc::new(PooledExecutor::new(inner, pool.clone())),
        pool,
    )
}

/// Runs `requests` futures from a plain thread at once and waits for all.
fn all<F>(requests: Vec<F>) -> Vec<F::Output>
where
    F: std::future::Future,
{
    workers::block_on_timeout(futures::future::join_all(requests), DEADLINE).unwrap()
}

#[test]
fn every_request_runs_on_a_worker() {
    let (recorder, executor, pool) = pooled(2);
    all(vec![
        executor.execute(QueryRequest::new("{ User { name } }"))
    ]);
    assert!(workers::block_on(executor.schema()).unwrap().is_ok());
    assert!(recorder.workers().iter().all(Option::is_some));
    pool.shutdown_blocking().unwrap();
}

/// Before routing by key, concurrent writes to one document ran at once on
/// whichever threads took them.
#[test]
fn writes_to_one_document_run_one_at_a_time() {
    let (recorder, executor, pool) = pooled(4);
    let write = r#"mutation { update_User(docID: "bae-1", input: {age: 3}) { _docID } }"#;
    let responses = all((0..16)
        .map(|_| executor.execute(QueryRequest::new(write)))
        .collect());
    assert!(responses.iter().all(|response| response.errors.is_empty()));
    assert_eq!(recorder.most.load(Ordering::SeqCst), 1);
    let ran: Vec<_> = recorder.workers();
    assert!(
        ran.windows(2).all(|pair| pair[0] == pair[1]),
        "one document's writes run on its worker: {ran:?}"
    );
    pool.shutdown_blocking().unwrap();
}

/// An explicit transaction's requests reach the inner executor from where
/// they are made: its registry, not this boundary, sends them to the worker
/// that owns the transaction.
#[test]
fn an_explicit_transactions_requests_pass_through_to_its_registry() {
    let (recorder, executor, pool) = pooled(2);
    let handle = workers::block_on(executor.begin_txn(false))
        .unwrap()
        .unwrap();
    workers::block_on(executor.execute_in_txn(QueryRequest::new("{ User { name } }"), &handle))
        .unwrap();
    workers::block_on(executor.commit_txn(&handle))
        .unwrap()
        .unwrap();
    assert_eq!(recorder.workers(), vec![None, None, None]);
    pool.shutdown_blocking().unwrap();
}

/// A task on a worker that submits a request and awaits it suspends; it
/// never blocks its worker on its own completion, even with one worker.
#[test]
fn a_worker_awaiting_its_own_submission_does_not_wait_on_itself() {
    let (_recorder, executor, pool) = pooled(1);
    let nested = Arc::clone(&executor);
    let response = pool
        .submit_and_wait(workers::Target::Worker(0), move || async move {
            nested.execute(QueryRequest::new("{ User { name } }")).await
        })
        .unwrap();
    assert!(response.errors.is_empty());
    assert_eq!(
        workers::block_on(async {
            pool.submit(workers::Target::Worker(0), || async {
                workers::block_on(async {})
            })
            .await
            .unwrap()
            .await
            .unwrap()
        })
        .unwrap(),
        Err(workers::Error::OnWorker),
        "a blocking wait on a worker is refused, not a deadlock"
    );
    pool.shutdown_blocking().unwrap();
}

#[test]
fn a_stopped_pool_answers_with_an_error_not_a_hang() {
    let (_recorder, executor, pool) = pooled(1);
    pool.shutdown_blocking().unwrap();
    let response = workers::block_on_timeout(
        executor.execute(QueryRequest::new("{ User { name } }")),
        DEADLINE,
    )
    .unwrap();
    assert!(!response.errors.is_empty());
}

/// Answers every request once `release` is set.
struct Gated {
    release: regolith::sync::Event,
}

#[async_trait]
impl QueryExecutor for Gated {
    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        self.release.wait().await;
        QueryResponse::success(
            serde_json::json!({ "ran_on": workers::current_worker().map(|worker| worker.index) }),
        )
    }

    async fn execute_in_txn(
        &self,
        request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.execute(request).await
    }

    async fn begin_txn(
        &self,
        _readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        Ok(TransactionHandle::new("t-1".into()))
    }

    async fn commit_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Ok(())
    }

    async fn discard_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Ok(())
    }

    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn schema(&self) -> Result<String> {
        Ok(String::new())
    }
}

/// A request whose caller is called back is admitted and left running: the
/// caller's wait ends before the response exists, and the response reaches
/// the callback on the worker that ran the request.
#[test]
fn a_called_back_request_answers_on_the_worker_that_ran_it() {
    let pool = workers::Pool::start(workers::Config::unplaced(2)).unwrap();
    let gated = Arc::new(Gated {
        release: regolith::sync::Event::new(),
    });
    let inner: Arc<dyn QueryExecutor> = gated.clone();
    let executor = PooledExecutor::new(inner, pool.clone());
    let answered = Arc::new(regolith::sync::Event::new());
    let answers = Arc::new(SegQueue::new());

    let (signal, store) = (Arc::clone(&answered), Arc::clone(&answers));
    workers::block_on_timeout(
        executor.execute_then(
            QueryRequest::new("{ User { name } }"),
            Box::new(move |response| {
                store.push((
                    workers::current_worker().map(|worker| worker.index),
                    response,
                ));
                signal.set();
            }),
        ),
        DEADLINE,
    )
    .unwrap();
    assert!(answers.is_empty(), "the caller waited only for admission");

    gated.release.set();
    workers::block_on_timeout(answered.wait(), DEADLINE).unwrap();
    let (answered_on, response) = answers.pop().unwrap();
    assert!(answered_on.is_some(), "the callback runs on a worker");
    let ran_on = response.data.unwrap()["ran_on"].as_u64();
    assert_eq!(ran_on, answered_on.map(|index| index as u64));
    pool.shutdown_blocking().unwrap();
}

/// A pool that cannot run the request drops its callback uncalled, and the
/// caller does not wait for an answer that will not come.
#[test]
fn a_called_back_request_the_pool_refuses_drops_its_callback() {
    let (recorder, executor, pool) = pooled(1);
    pool.shutdown_blocking().unwrap();
    let called = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&called);
    workers::block_on_timeout(
        executor.execute_then(
            QueryRequest::new("{ User { name } }"),
            Box::new(move |_| {
                count.fetch_add(1, Ordering::SeqCst);
            }),
        ),
        DEADLINE,
    )
    .unwrap();
    assert_eq!(called.load(Ordering::SeqCst), 0);
    assert!(recorder.workers().is_empty());
}
