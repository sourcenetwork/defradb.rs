//! What other threads see of a worker: its two bounded inboxes, the admission
//! that bounds them, its run-queue signal, its heartbeat and its counters.
//!
//! Work bound to this worker (an explicit transaction's requests) and work
//! that may run anywhere (queries, least-loaded work, document-affinity
//! jobs) arrive in separate inboxes, so a stuck worker's movable work can be
//! taken by another thread without ever reordering its bound work.
//!
//! Admission counts items and bytes per worker, held from submission until
//! the task finishes, so a worker's queued plus running work stays within its
//! bounds; a submitter waits for a permit instead of growing a queue.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{fence, AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use regolith::sync::{ArrayQueue, Semaphore};

use super::heartbeat::Heartbeat;
use super::signal::Signal;
use crate::completion::{self, Completion, Fail};
use crate::os::ContextSwitches;
use crate::WorkerId;

/// A task's future, built on the worker that runs it.
pub(crate) type LocalFuture = Pin<Box<dyn Future<Output = ()>>>;

/// Builds a task's future on its worker; crosses threads, the future does not.
pub(crate) type Start = Box<dyn FnOnce() -> LocalFuture + Send>;

/// One submission on its way to a worker.
pub(crate) struct Job {
    pub start: Start,
    pub fail: Arc<dyn Fail + Send + Sync>,
    pub name: &'static str,
    /// Document affinity: one task per key at a time on the worker.
    pub key: Option<u64>,
    /// Bound to this worker (its owner); never moved to another.
    pub bound: bool,
    pub admitted: Admitted,
}

impl Job {
    /// A job that builds its future with `make` on the worker and delivers
    /// the output to the returned completion.
    pub(crate) fn new<F, Fut>(
        make: F,
        name: &'static str,
        key: Option<u64>,
        bound: bool,
        admitted: Admitted,
    ) -> (Job, Completion<Fut::Output>)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let (sender, completion) = completion::channel();
        let fail: Arc<dyn Fail + Send + Sync> = sender.shared();
        let start: Start = Box::new(move || {
            Box::pin(async move {
                // Checked at its first poll, when it starts, not when it is
                // taken from the inbox: a worker takes a batch at once, and
                // a waiter that gives up meanwhile is still in time.
                if sender.is_cancelled() {
                    return;
                }
                let slot = sender.into_slot();
                slot.send(make().await)
            })
        });
        (
            Job {
                start,
                fail,
                name,
                key,
                bound,
                admitted,
            },
            completion,
        )
    }
}

pub(crate) enum Message {
    Job(Job),
    /// Finish the admitted work, then stop.
    Shutdown,
}

/// A worker's admission permits for one job, given back when dropped.
pub(crate) struct Admitted {
    worker: Arc<WorkerShared>,
    bytes: usize,
}

impl Admitted {
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// The worker the permits belong to, where the job goes.
    pub(crate) fn worker(&self) -> &Arc<WorkerShared> {
        &self.worker
    }
}

impl Drop for Admitted {
    fn drop(&mut self) {
        self.worker.items.add_permits(1);
        self.worker.bytes.add_permits(self.bytes);
    }
}

/// Monotonic counters a worker keeps of itself.
#[derive(Default)]
pub(crate) struct Counters {
    pub polls: AtomicU64,
    pub tasks: AtomicU64,
    pub panics: AtomicU64,
    pub long_polls: AtomicU64,
    pub moved_out: AtomicU64,
    pub repins: AtomicU64,
    pub priority_resets: AtomicU64,
}

/// Adds one to a counter only its owner writes: a plain load and store, no
/// locked instruction.
pub(crate) fn bump(counter: &AtomicU64) {
    counter.store(counter.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
}

/// One worker's counters and load, read from any thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkerStats {
    /// Task polls run.
    pub polls: u64,
    /// Tasks started.
    pub tasks: u64,
    /// Tasks that panicked.
    pub panics: u64,
    /// Polls that ran past the stuck limit.
    pub long_polls: u64,
    /// Queued jobs moved off this worker while it was stuck.
    pub moved_out: u64,
    /// Times this worker found itself off its CPUs and pinned itself again.
    pub repins: u64,
    /// Times this worker's priority was found changed from outside.
    pub priority_resets: u64,
    /// Submissions admitted and not finished (queued plus running).
    pub load: usize,
    /// Whether a poll is running past the stuck limit now.
    pub stuck: bool,
    /// How often the worker's thread was switched out so far; `None` where
    /// the platform does not say (only Linux does, per thread).
    pub switches: Option<ContextSwitches>,
}

pub(crate) struct WorkerShared {
    pub id: WorkerId,
    pub signal: Arc<Signal>,
    bound: ArrayQueue<Message>,
    free: ArrayQueue<Message>,
    items: Semaphore,
    bytes: Semaphore,
    items_cap: usize,
    bytes_cap: usize,
    pub heartbeat: Heartbeat,
    /// A poll is running past the limit; routing goes around this worker.
    pub stuck: AtomicBool,
    /// The task last reported stuck, so two monitors report it once.
    pub reported: AtomicU64,
    /// Stop now, dropping what is left.
    pub abandon: AtomicBool,
    /// The worker has stopped: whoever finds a message after this fails it.
    exited: AtomicBool,
    pub counters: Counters,
    /// The OS id of the thread running the worker, 0 until it starts.
    thread: AtomicU64,
}

impl WorkerShared {
    pub(crate) fn new(
        id: WorkerId,
        signal: Arc<Signal>,
        items: usize,
        bytes: usize,
    ) -> WorkerShared {
        // A semaphore holds at most `MAX_PERMITS`, which is far smaller on a
        // 32-bit target (wasm32): a budget past it is the same as no budget.
        let items = items.min(Semaphore::MAX_PERMITS);
        let bytes = bytes.min(Semaphore::MAX_PERMITS);
        WorkerShared {
            id,
            signal,
            // One slot more than admission allows, for the shutdown message.
            bound: ArrayQueue::new(items + 1),
            free: ArrayQueue::new(items),
            items: Semaphore::new(items),
            bytes: Semaphore::new(bytes),
            items_cap: items,
            bytes_cap: bytes,
            heartbeat: Heartbeat::new(),
            stuck: AtomicBool::new(false),
            reported: AtomicU64::new(u64::MAX),
            abandon: AtomicBool::new(false),
            exited: AtomicBool::new(false),
            counters: Counters::default(),
            thread: AtomicU64::new(0),
        }
    }

    /// Records the calling thread as the one running this worker.
    pub(crate) fn bind_thread(&self) {
        if let Some(id) = crate::os::system().thread_id() {
            self.thread.store(id, Ordering::Relaxed);
        }
    }

    /// The context switches of the worker's thread so far, read from the OS.
    fn switches(&self) -> Option<ContextSwitches> {
        match self.thread.load(Ordering::Relaxed) {
            0 => None,
            id => crate::os::system().context_switches(id).ok(),
        }
    }

    /// Waits for room for one job of `bytes` (capped at the worker's byte
    /// bound, so a large job still fits alone). Cancel-safe: dropped before
    /// it completes, it holds nothing.
    pub(crate) async fn admit(self: &Arc<Self>, bytes: usize) -> Admitted {
        let bytes = bytes.clamp(1, self.bytes_cap);
        let item = self.items.acquire(1).await;
        let room = self.bytes.acquire(bytes).await;
        item.forget();
        room.forget();
        Admitted {
            worker: self.clone(),
            bytes,
        }
    }

    /// Room for one job of `bytes` if there is some now.
    pub(crate) fn try_admit(self: &Arc<Self>, bytes: usize) -> Option<Admitted> {
        let bytes = bytes.clamp(1, self.bytes_cap);
        let item = self.items.try_acquire(1)?;
        let room = self.bytes.try_acquire(bytes)?;
        item.forget();
        room.forget();
        Some(Admitted {
            worker: self.clone(),
            bytes,
        })
    }

    /// Submissions admitted and not finished.
    pub(crate) fn load(&self) -> usize {
        self.items_cap
            .saturating_sub(self.items.available_permits())
    }

    /// Queues an admitted job and wakes the worker if it parks. The job's
    /// admission reserved its room, so the push finds space.
    pub(crate) fn push(&self, job: Job) {
        self.push_all(std::iter::once(job));
    }

    /// Queues admitted jobs, then wakes the worker if it parks: once for all
    /// of them, which it finds together.
    pub(crate) fn push_all(&self, jobs: impl IntoIterator<Item = Job>) {
        for job in jobs {
            let inbox = if job.bound { &self.bound } else { &self.free };
            if let Err(Message::Job(job)) = inbox.push(Message::Job(job)) {
                // Unreachable while admission holds; failing the job beats losing it.
                job.fail.fail(crate::Error::ShutDown);
            }
        }
        self.signal.notify();
        // Pairs with `exit`: either the worker's last drain sees this job, or
        // this sees the worker gone and fails what is left.
        fence(Ordering::SeqCst);
        if self.exited.load(Ordering::Relaxed) {
            self.drain_orphans();
        }
    }

    /// The next message, alternating which inbox goes first by `free_first`.
    pub(crate) fn take(&self, free_first: bool) -> Option<Message> {
        if free_first {
            self.free.pop().or_else(|| self.bound.pop())
        } else {
            self.bound.pop().or_else(|| self.free.pop())
        }
    }

    /// The next movable job, for a monitor rerouting a stuck worker's work.
    pub(crate) fn take_free(&self) -> Option<Job> {
        match self.free.pop()? {
            Message::Job(job) => Some(job),
            Message::Shutdown => None,
        }
    }

    pub(crate) fn has_messages(&self) -> bool {
        !self.bound.is_empty() || !self.free.is_empty()
    }

    /// Tells the worker to finish its admitted work and stop.
    pub(crate) fn shutdown(&self) {
        let _ = self.bound.push(Message::Shutdown);
        self.signal.notify();
    }

    /// Tells the worker to stop now, failing what it has not finished.
    pub(crate) fn abandon(&self) {
        self.abandon.store(true, Ordering::Release);
        self.signal.notify();
    }

    /// The worker has stopped: what reaches its inboxes from now on is failed.
    pub(crate) fn exit(&self) {
        self.exited.store(true, Ordering::Relaxed);
        fence(Ordering::SeqCst);
        self.drain_orphans();
    }

    /// Drops every queued message; each job's completion fails with
    /// [`Error::ShutDown`](crate::Error::ShutDown) and its admission is given
    /// back.
    pub(crate) fn drain_orphans(&self) {
        while let Some(message) = self.take(false) {
            drop(message);
        }
    }

    pub(crate) fn stats(&self) -> WorkerStats {
        let read = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        WorkerStats {
            polls: read(&self.counters.polls),
            tasks: read(&self.counters.tasks),
            panics: read(&self.counters.panics),
            long_polls: read(&self.counters.long_polls),
            moved_out: read(&self.counters.moved_out),
            repins: read(&self.counters.repins),
            priority_resets: read(&self.counters.priority_resets),
            load: self.load(),
            stuck: self.stuck.load(Ordering::Relaxed),
            switches: self.switches(),
        }
    }
}
