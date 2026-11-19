//! The engine-owned retry: one call runs an attempt in a transaction regolith
//! owns and commits, and runs it again after each conflict, up to the
//! policy's budget.
//!
//! An attempt sees a [`RegolithRetryTxn`]: the reads and writes of the
//! engine transaction, and nothing that could commit it or let it escape.
//! What the attempt registered for after its end ([`PostCommit`]) is bound to
//! the commit-receipt stream before regolith commits it; an attempt that
//! loses lets go of its place and runs its abort callbacks, and only the
//! winner's commit callbacks run.

#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

use std::sync::Arc;

use async_trait::async_trait;
use regolith::sync::ArrayQueue;
use regolith::OptimisticTransactionDb;

use super::event_order::EventOrder;
use super::post_commit::PostCommit;
use super::transaction::RegolithTxn;
use super::worker_io::readied;
use crate::corekv::{
    Error, Result, RetryAttempt, RetryAttemptError, RetryAttemptFuture, RetryAttemptIO,
    RetryAttemptValue, RetryOptions, RetryTxn, TxnCallbacks,
};

/// What one attempt hands the next: the caller's attempt, and the work the
/// latest attempt registered. Exactly one attempt runs at a time, so it
/// takes the baton when it starts and gives it back when it ends.
struct Baton {
    attempt: RetryAttempt,
    /// regolith runs exactly one outcome per attempt: an attempt that lost
    /// already let go of its place in the stream through its own
    /// `on_abort`, and its abort callbacks run when the next one starts.
    latest: Option<PostCommit>,
}

/// Gives `attempts` its higher-ranked signature: each call's future borrows
/// only that call's transaction, and is `Send` wherever futures must be,
/// which an async closure cannot yet promise.
fn attempts<F>(attempts: F) -> F
where
    F: for<'txn, 'lost> FnMut(
        &'txn mut regolith::Transaction,
        Option<&'lost regolith::Conflict>,
    ) -> RetryAttemptFuture<'txn>,
{
    attempts
}

/// Runs `attempt` in engine transactions of `db` until one commits, an
/// attempt fails, or the policy's attempts are spent.
pub(super) async fn transact(
    db: &Arc<OptimisticTransactionDb>,
    io: &super::worker_io::StoreIo,
    queue: Option<regolith::QueueId>,
    event_order: &Arc<EventOrder>,
    options: RetryOptions,
    attempt: RetryAttempt,
) -> std::result::Result<RetryAttemptValue, RetryAttemptError> {
    let policy = regolith::RetryPolicy::new(options.max_attempts.max(1));
    let baton = Arc::new(ArrayQueue::new(1));
    let _ = baton.push(Baton {
        attempt,
        latest: None,
    });
    let run = attempts({
        let (baton, event_order) = (Arc::clone(&baton), Arc::clone(event_order));
        move |txn, lost| {
            let previous_conflict = lost.map(crate::corekv::TransactionConflict::from);
            let (baton, event_order) = (Arc::clone(&baton), Arc::clone(&event_order));
            Box::pin(async move {
                let Some(mut held) = baton.pop() else {
                    return Err(RetryAttemptError::Storage(Error::Other(
                        "a retry attempt started while another was running".into(),
                    )));
                };
                let value = run_attempt(&mut held, txn, previous_conflict, &event_order).await;
                // Cannot be full: this attempt took the one baton.
                let _ = baton.push(held);
                value
            })
        }
    });
    // On a pool the attempts do their I/O on a worker's queue, polled
    // between its tasks. Without one the task drives a queue of its own and
    // never parks its thread, so the executor it runs on keeps scheduling it.
    let outcome = match queue {
        Some(queue) => db.transact_async(queue, &policy, run).await,
        None => {
            let mut queue = io.own_queue(db);
            let future = db.transact_async(queue.id(), &policy, run);
            let outcome = super::worker_io::drive(&mut queue, future).await;
            io.keep_queue(queue);
            outcome
        }
    };
    let committed = outcome.is_ok();
    if let Some(latest) = baton.pop().and_then(|held| held.latest) {
        if committed {
            latest.committed().await;
        } else {
            latest.failed().await;
        }
    }
    match outcome {
        Ok((value, _receipt)) => Ok(value),
        Err(regolith::TransactError::Closure(error)) => Err(error),
        Err(regolith::TransactError::Exhausted(conflict)) => Err(RetryAttemptError::Exhausted(
            crate::corekv::TransactionConflict::from(&conflict),
        )),
        Err(regolith::TransactError::Engine(error)) => Err(RetryAttemptError::Storage(
            super::transaction::map_txn_error(error),
        )),
        Err(_other) => Err(RetryAttemptError::Storage(Error::Backend(
            "unknown regolith transaction retry failure".into(),
        ))),
    }
}

/// One attempt: the previous attempt's work fails, `held`'s attempt runs in
/// `txn`, and what it registered for after its end is bound to the stream.
async fn run_attempt(
    held: &mut Baton,
    txn: &mut regolith::Transaction,
    previous_conflict: Option<crate::corekv::TransactionConflict>,
    event_order: &Arc<EventOrder>,
) -> std::result::Result<RetryAttemptValue, RetryAttemptError> {
    if previous_conflict.is_some() {
        telemetry::record_retry_attempt(telemetry::RetryLayer::Engine);
    }
    if let Some(previous) = held.latest.take() {
        previous.failed().await;
    }
    let mut retry_txn = RegolithRetryTxn {
        previous_conflict,
        txn,
        attempt_io_active: false,
        callbacks: TxnCallbacks::default(),
    };
    let mut value = held.attempt.run(&mut retry_txn).await;
    if value.is_ok() && retry_txn.attempt_io_active {
        value = Err(RetryAttemptError::Storage(Error::Other(
            "retry attempt returned with scoped IO still active".into(),
        )));
    }
    if value.is_ok() {
        let mut callbacks = std::mem::take(&mut retry_txn.callbacks);
        match callbacks.run_before_commit() {
            Ok(()) => {
                held.latest = Some(PostCommit::bind(event_order, retry_txn.txn, callbacks));
            }
            Err(error) => {
                let (_, aborted) = callbacks.into_ends();
                aborted.run().await;
                value = Err(RetryAttemptError::Storage(error));
            }
        }
    }
    value
}

struct RegolithRetryTxn<'a> {
    previous_conflict: Option<crate::corekv::TransactionConflict>,
    txn: &'a mut regolith::Transaction,
    attempt_io_active: bool,
    callbacks: TxnCallbacks,
}

impl RegolithRetryTxn<'_> {
    fn ensure_no_active_attempt_io(&self) -> Result<()> {
        if self.attempt_io_active {
            return Err(Error::Other(
                "raw retry operations are unavailable while scoped IO is active".into(),
            ));
        }
        Ok(())
    }
}

impl crate::corekv::private::Sealed for RegolithRetryTxn<'_> {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl RetryTxn for RegolithRetryTxn<'_> {
    fn previous_conflict(&self) -> Option<crate::corekv::TransactionConflict> {
        self.previous_conflict
    }

    async fn get(&self, key: &[u8]) -> Result<Option<bytes::Bytes>> {
        self.ensure_no_active_attempt_io()?;
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        let txn: &regolith::Transaction = self.txn;
        readied!(txn.get_slice(key))
            .map(|value| value.map(bytes::Bytes::from_owner))
            .map_err(super::transaction::map_txn_error)
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.ensure_no_active_attempt_io()?;
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        let txn: &regolith::Transaction = self.txn;
        readied!(txn.get_slice(key))
            .map(|value| value.is_some())
            .map_err(super::transaction::map_txn_error)
    }

    async fn set(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        self.ensure_no_active_attempt_io()?;
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        let txn: &regolith::Transaction = self.txn;
        readied!(txn.put(key, value)).map_err(super::transaction::map_txn_error)?;
        Ok(())
    }

    async fn delete(&mut self, key: &[u8]) -> Result<()> {
        self.ensure_no_active_attempt_io()?;
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        let txn: &regolith::Transaction = self.txn;
        readied!(txn.delete(key)).map_err(super::transaction::map_txn_error)?;
        Ok(())
    }

    fn begin_attempt_io(&mut self) -> Result<Box<dyn RetryAttemptIO>> {
        if self.attempt_io_active {
            return Err(Error::Other("scoped retry IO is already active".into()));
        }
        // The attempt's transaction moves to the scoped IO; what regolith
        // holds meanwhile is an empty stand-in that registers nothing.
        let actual = Box::new(self.txn.take());
        self.attempt_io_active = true;
        Ok(Box::new(RegolithTxn::scoped_attempt(actual)))
    }

    fn finish_attempt_io(
        &mut self,
        io: Box<dyn RetryAttemptIO>,
        callbacks: TxnCallbacks,
    ) -> Result<()> {
        let io = io
            .into_any()
            .downcast::<RegolithTxn>()
            .map_err(|_| Error::Other("retry IO belongs to a different backend".into()))?;
        let actual = io.into_scoped_engine_txn()?;
        *self.txn = *actual;
        self.callbacks = callbacks;
        self.attempt_io_active = false;
        Ok(())
    }
}

#[cfg(test)]
mod retry_cold_io_tests {
    use super::*;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::backends::RegolithStore;
    use crate::corekv::Store;

    /// Pending until the executor has polled the task that awaits the retry
    /// again: an attempt driven by re-polling it inside one poll of that task
    /// never sees the count move.
    struct YieldToExecutor {
        task_polls: Arc<AtomicUsize>,
        seen: Option<usize>,
        stalled: usize,
    }

    impl std::future::Future for YieldToExecutor {
        type Output = ();

        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<()> {
            let polls = self.task_polls.load(Ordering::SeqCst);
            if self.seen.is_some_and(|seen| seen != polls) {
                return std::task::Poll::Ready(());
            }
            self.seen = Some(polls);
            self.stalled += 1;
            assert!(
                self.stalled < 64,
                "the attempt was re-polled without its task being polled"
            );
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }

    struct YieldingAttempt {
        task_polls: Arc<AtomicUsize>,
    }
    impl crate::corekv::RetryAttemptFn for YieldingAttempt {
        fn run<'a>(
            &'a mut self,
            _retry: &'a mut dyn RetryTxn,
        ) -> crate::corekv::RetryAttemptFuture<'a> {
            let task_polls = Arc::clone(&self.task_polls);
            Box::pin(async move {
                // More yields than any one poll budget. The engine queue must
                // finish an attempt that keeps handing the thread back, one
                // poll of its task at a time.
                for _ in 0..1024 {
                    YieldToExecutor {
                        task_polls: Arc::clone(&task_polls),
                        seen: None,
                        stalled: 0,
                    }
                    .await;
                }
                Ok(Box::new(()) as RetryAttemptValue)
            })
        }
    }

    /// Off a worker, so the attempt is driven by the task that awaits it.
    #[test]
    fn retry_queue_completes_an_attempt_that_yields_repeatedly() {
        let task_polls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&task_polls);
        workers::block_on_timeout(
            async move {
                let store = RegolithStore::in_memory().unwrap();
                let retry = store.transact_retry(
                    RetryOptions::new(1),
                    Box::new(YieldingAttempt { task_polls }),
                );
                let mut retry = std::pin::pin!(retry);
                std::future::poll_fn(|cx| {
                    counted.fetch_add(1, Ordering::SeqCst);
                    retry.as_mut().poll(cx)
                })
                .await
                .unwrap();
            },
            std::time::Duration::from_secs(30),
        )
        .unwrap()
    }

    struct ColdReadAttempt;
    impl crate::corekv::RetryAttemptFn for ColdReadAttempt {
        fn run<'a>(
            &'a mut self,
            retry: &'a mut dyn RetryTxn,
        ) -> crate::corekv::RetryAttemptFuture<'a> {
            Box::pin(async move {
                let mut io = retry.begin_attempt_io()?;
                assert_eq!(io.get(b"cold/00").await?.unwrap().len(), 16384);
                assert!(io.has(b"cold/01").await?);
                assert_eq!(io.get_size(b"cold/02").await?, Some(16384));
                assert_eq!(io.get_parts(b"cold/03", &[]).await?.unwrap().len(), 16384);
                for validated in [false, true] {
                    let mut options =
                        crate::corekv::IterOptions::new().with_prefix(b"cold/".to_vec());
                    if validated {
                        options = options.with_validation(true);
                    }
                    let mut iter = io.iterator(options).await?;
                    let mut count = 0;
                    while let Some(pair) = iter.next().await? {
                        assert_eq!(pair.value.len(), 16384);
                        count += 1;
                    }
                    assert_eq!(count, 24);
                    iter.close().await?;
                    drop(iter);
                }
                io.set(b"cold-result", b"complete").await?;
                retry.finish_attempt_io(io, TxnCallbacks::default())?;
                Ok(Box::new(()) as RetryAttemptValue)
            })
        }
    }

    #[test]
    fn scoped_retry_io_reads_cold_sst_points_and_scans() {
        crate::backends::test_suite::on_workers(|| async {
            let directory = tempfile::tempdir().unwrap();
            let store = RegolithStore::open(directory.path()).unwrap();
            let mut txn = store.new_txn(false).await.unwrap();
            for n in 0..24 {
                txn.set(format!("cold/{n:02}").as_bytes(), &vec![n; 16384])
                    .await
                    .unwrap();
            }
            txn.commit().await.unwrap();
            store.engine().db().flush().unwrap();
            store.close().await.unwrap();
            drop(store);
            let store = RegolithStore::open(directory.path()).unwrap();
            store
                .transact_retry(RetryOptions::new(2), Box::new(ColdReadAttempt))
                .await
                .unwrap();
            let txn = store.new_txn(true).await.unwrap();
            assert_eq!(
                txn.get(b"cold-result").await.unwrap().as_deref(),
                Some(b"complete".as_slice())
            );
            txn.discard();
            store.close().await.unwrap();
        });
    }
}

#[cfg(test)]
mod retry_read_tests {
    use super::*;
    use crate::backends::RegolithStore;
    use crate::corekv::{OnAbort, OnCommit, Store, TxnCallback};
    use regolith::TxnOptions;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A compare-and-set whose condition finds nothing to change, while
    /// another writer updates the key it read before it commits.
    struct ConditionalNoop {
        store: RegolithStore,
    }
    impl crate::corekv::RetryAttemptFn for ConditionalNoop {
        fn run<'a>(
            &'a mut self,
            retry: &'a mut dyn RetryTxn,
        ) -> crate::corekv::RetryAttemptFuture<'a> {
            Box::pin(async move {
                assert_eq!(retry.get(b"flag").await?.as_deref(), Some(&b"old"[..]));
                assert!(retry.has(b"flag").await?);
                let other = self.store.engine().begin(&TxnOptions::new());
                other.put(b"flag", b"new").unwrap();
                other.commit().unwrap();
                Ok(Box::new(()) as RetryAttemptValue)
            })
        }
    }

    #[test]
    fn a_write_free_attempt_and_an_update_of_what_it_read_both_commit() {
        crate::backends::test_suite::on_workers(|| async {
            let store = RegolithStore::in_memory().unwrap();
            let mut seed = store.new_txn(false).await.unwrap();
            seed.set(b"flag", b"old").await.unwrap();
            seed.commit().await.unwrap();
            store
                .transact_retry(
                    RetryOptions::new(1),
                    Box::new(ConditionalNoop {
                        store: store.clone(),
                    }),
                )
                .await
                .unwrap();
            let read = store.new_txn(true).await.unwrap();
            assert_eq!(
                read.get(b"flag").await.unwrap().as_deref(),
                Some(&b"new"[..])
            );
            read.discard();
        });
    }
    fn counting(count: &Arc<AtomicUsize>) -> TxnCallback {
        let count = Arc::clone(count);
        Box::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        })
    }

    /// An attempt that writes after a read another writer changes before
    /// its first run commits, and the callbacks each run registers.
    struct LosesOnce {
        store: RegolithStore,
        runs: usize,
        committed: Arc<AtomicUsize>,
        aborted: Arc<AtomicUsize>,
    }
    impl crate::corekv::RetryAttemptFn for LosesOnce {
        fn run<'a>(
            &'a mut self,
            retry: &'a mut dyn RetryTxn,
        ) -> crate::corekv::RetryAttemptFuture<'a> {
            Box::pin(async move {
                self.runs += 1;
                let mut io = retry.begin_attempt_io()?;
                assert!(io.get(b"flag").await?.is_some());
                io.set(b"out", b"v").await?;
                if self.runs == 1 {
                    let other = self.store.engine().begin(&TxnOptions::new());
                    other.put(b"flag", b"changed").unwrap();
                    other.commit().unwrap();
                }
                let mut callbacks = TxnCallbacks::default();
                callbacks.on_commit(OnCommit::Sync(counting(&self.committed)))?;
                callbacks.on_abort(OnAbort::Sync(counting(&self.aborted)));
                retry.finish_attempt_io(io, callbacks)?;
                Ok(Box::new(()) as RetryAttemptValue)
            })
        }
    }

    /// Each attempt ends once: the one that lost runs its abort callbacks,
    /// the one that won its commit callbacks.
    #[test]
    fn an_attempt_that_loses_runs_its_abort_callbacks_and_the_winner_its_commit_ones() {
        crate::backends::test_suite::on_workers(|| async {
            let store = RegolithStore::in_memory().unwrap();
            let mut seed = store.new_txn(false).await.unwrap();
            seed.set(b"flag", b"old").await.unwrap();
            seed.commit().await.unwrap();
            let (committed, aborted) =
                (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
            let attempt = LosesOnce {
                store: store.clone(),
                runs: 0,
                committed: Arc::clone(&committed),
                aborted: Arc::clone(&aborted),
            };
            store
                .transact_retry(RetryOptions::new(2), Box::new(attempt))
                .await
                .unwrap();
            assert_eq!(committed.load(Ordering::SeqCst), 1);
            assert_eq!(aborted.load(Ordering::SeqCst), 1);
        });
    }
}

#[cfg(test)]
mod retry_order_tests {
    use super::*;
    use crate::backends::RegolithStore;
    use crate::corekv::{OnCommit, Store};
    use regolith::sync::Notify;
    use regolith::sync::SegQueue;

    struct OrderedAttempt {
        events: Arc<SegQueue<u8>>,
        entered: Arc<Notify>,
        release: Arc<Notify>,
    }
    impl crate::corekv::RetryAttemptFn for OrderedAttempt {
        fn run<'a>(
            &'a mut self,
            retry: &'a mut dyn RetryTxn,
        ) -> crate::corekv::RetryAttemptFuture<'a> {
            Box::pin(async move {
                let mut io = retry.begin_attempt_io()?;
                io.set(b"ordered-retry", b"durable").await?;
                let mut callbacks = TxnCallbacks::default();
                let entered = self.entered.clone();
                let release = self.release.clone();
                callbacks.on_commit(OnCommit::OrderedAsync(Box::new(move || {
                    Box::pin(async move {
                        entered.notify_one();
                        release.notified().await;
                        Ok(())
                    })
                })))?;
                let events = self.events.clone();
                callbacks.on_commit(OnCommit::Ordered(Box::new(move || events.push(1))))?;
                retry.finish_attempt_io(io, callbacks)?;
                Ok(Box::new(()) as RetryAttemptValue)
            })
        }
    }

    #[test]
    fn regular_commit_waits_for_earlier_retry_postcommit_hook() {
        crate::backends::test_suite::on_workers(|| async {
            let store = RegolithStore::in_memory().unwrap();
            let events = Arc::new(SegQueue::new());
            let entered = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());
            let attempt = OrderedAttempt {
                events: events.clone(),
                entered: entered.clone(),
                release: release.clone(),
            };
            let retry_store = store.clone();
            let retry_task = workers::spawn(None, "test", move || async move {
                retry_store
                    .transact_retry(RetryOptions::new(2), Box::new(attempt))
                    .await
                    .unwrap();
            })
            .unwrap();
            workers::timeout(std::time::Duration::from_secs(5), entered.notified())
                .await
                .unwrap();
            let mut regular = store.new_txn(false).await.unwrap();
            regular.set(b"ordered-regular", b"durable").await.unwrap();
            let out = events.clone();
            regular
                .on_commit(OnCommit::Ordered(Box::new(move || out.push(2))))
                .unwrap();
            regular.commit().await.unwrap();
            assert!(events.is_empty());
            release.notify_one();
            retry_task.await.unwrap();
            assert_eq!(
                std::iter::from_fn(|| events.pop()).collect::<Vec<_>>(),
                vec![1, 2]
            );
        });
    }
}
