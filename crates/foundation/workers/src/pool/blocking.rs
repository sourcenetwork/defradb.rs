//! The blocking pool: a small, bounded set of lowest-priority threads for
//! code that has to block (a synchronous library, a C callback), so it never
//! stalls a worker. Bounded twice: the threads, and the jobs queued for them,
//! a full queue making `spawn_blocking` wait. The pool counts every job, so
//! its use is visible and can shrink over time.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{fence, AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use regolith::sync::{ArrayQueue, Semaphore};

use super::placer::Placer;
use crate::completion::{self, Completion, Fail};
use crate::config::ThreadClass;
use crate::Error;

type BlockingJob = Box<dyn FnOnce() + Send>;

pub(crate) struct Blocking {
    jobs: ArrayQueue<BlockingJob>,
    /// Free queue slots: a submitter waits on it when the queue is full.
    room: Semaphore,
    /// Queued jobs: an idle thread waits on it.
    ready: Semaphore,
    closing: AtomicBool,
    threads: usize,
    queue: usize,
    /// Threads still serving; the last to stop fails what is left queued.
    live: AtomicUsize,
    handles: ArrayQueue<std::thread::JoinHandle<()>>,
}

impl Blocking {
    /// Starts `threads` threads with `queue` job slots.
    pub(crate) fn start(
        threads: usize,
        queue: usize,
        name: &str,
        placer: Arc<Placer>,
    ) -> Result<Arc<Blocking>, Error> {
        let queue = queue.min(Semaphore::MAX_PERMITS);
        let blocking = Arc::new(Blocking {
            jobs: ArrayQueue::new(queue),
            room: Semaphore::new(queue),
            ready: Semaphore::new(0),
            closing: AtomicBool::new(false),
            threads,
            queue,
            live: AtomicUsize::new(threads),
            handles: ArrayQueue::new(threads),
        });
        for index in 0..threads {
            let pool = blocking.clone();
            let placer = placer.clone();
            let handle = std::thread::Builder::new()
                .name(format!("{name}-blocking-{index}"))
                .spawn(move || {
                    placer.place(ThreadClass::Blocking);
                    pool.serve();
                    pool.live.fetch_sub(1, Ordering::SeqCst);
                    pool.drain_orphans();
                })
                .map_err(|e| Error::Spawn(e.to_string()));
            match handle {
                Ok(handle) => {
                    let _ = blocking.handles.push(handle);
                }
                Err(e) => {
                    blocking.live.fetch_sub(threads - index, Ordering::SeqCst);
                    blocking.close();
                    return Err(e);
                }
            }
        }
        Ok(blocking)
    }

    /// Queues `f` for a blocking thread, waiting while the queue is full.
    pub(crate) async fn spawn<F, T>(&self, f: F) -> Result<Completion<T>, Error>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        if self.closing.load(Ordering::Acquire) {
            return Err(Error::ShutDown);
        }
        let slot = self.room.acquire(1).await;
        let (sender, completion) = completion::channel();
        let fail = sender.shared();
        let job: BlockingJob = Box::new(move || match panic::catch_unwind(AssertUnwindSafe(f)) {
            Ok(value) => sender.send(value),
            Err(payload) => {
                let message = crate::worker::panic_message(payload.as_ref());
                fail.fail(Error::Panicked {
                    task: std::any::type_name::<F>(),
                    message,
                });
            }
        });
        if self.jobs.push(job).is_err() {
            return Err(Error::ShutDown);
        }
        slot.forget();
        self.ready.add_permits(1);
        // Pairs with a stopping thread: either a thread still serves and takes
        // this job, or this sees none left and fails it.
        fence(Ordering::SeqCst);
        if self.live.load(Ordering::Relaxed) == 0 {
            self.drain_orphans();
        }
        Ok(completion)
    }

    /// Drops every queued job once no thread is left to run it: each
    /// completion fails with `ShutDown`.
    fn drain_orphans(&self) {
        if self.live.load(Ordering::SeqCst) != 0 {
            return;
        }
        while let Some(job) = self.jobs.pop() {
            self.room.add_permits(1);
            drop(job);
        }
    }

    /// A blocking thread's loop: take a job, run it, until closed.
    fn serve(&self) {
        loop {
            match crate::completion::block_on(self.ready.acquire(1)) {
                Ok(permit) => permit.forget(),
                Err(_) => return,
            }
            match self.jobs.pop() {
                Some(job) => {
                    self.room.add_permits(1);
                    job();
                }
                None if self.closing.load(Ordering::Acquire) => return,
                None => {}
            }
        }
    }

    /// Jobs queued and not yet taken by a thread.
    pub(crate) fn queued(&self) -> usize {
        self.queue.saturating_sub(self.room.available_permits())
    }

    /// Lets the threads finish the queued jobs and stop.
    pub(crate) fn close(&self) {
        if !self.closing.swap(true, Ordering::AcqRel) {
            self.ready.add_permits(self.threads);
        }
    }

    /// Joins the threads that have stopped; the rest are left running.
    pub(crate) fn join_finished(&self) {
        let mut running = Vec::new();
        while let Some(handle) = self.handles.pop() {
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                running.push(handle);
            }
        }
        running.into_iter().for_each(|handle| {
            let _ = self.handles.push(handle);
        });
    }
}
