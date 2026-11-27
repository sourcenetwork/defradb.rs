//! The cross-thread face of a worker's executor: the run queue any thread
//! pushes a woken task onto, and the parking protocol that wakes the worker
//! only while it sleeps, so a busy worker is never woken.

use std::sync::atomic::{fence, AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::task::{Wake, Waker};

use regolith::sync::SegQueue;

use super::slab::Key;

const RUNNING: u8 = 0;
const PARKED: u8 = 1;
const NOTIFIED: u8 = 2;

/// How a parked worker is woken.
pub(crate) enum Parker {
    /// A worker thread; set by the thread itself when it starts.
    #[cfg(not(target_family = "wasm"))]
    Thread(regolith::sync::OnceCell<std::thread::Thread>),
    /// A caller's event loop, which polls the worker when woken.
    Waker(Waker),
}

impl Parker {
    fn unpark(&self) {
        match self {
            #[cfg(not(target_family = "wasm"))]
            Parker::Thread(thread) => {
                if let Some(thread) = thread.get() {
                    thread.unpark();
                }
            }
            Parker::Waker(waker) => waker.wake_by_ref(),
        }
    }
}

/// A worker's run queue and park state.
pub(crate) struct Signal {
    ready: SegQueue<u64>,
    state: AtomicU8,
    parker: Parker,
}

impl Signal {
    pub(crate) fn new(parker: Parker) -> Signal {
        Signal {
            ready: SegQueue::new(),
            state: AtomicU8::new(RUNNING),
            parker,
        }
    }

    /// The thread that parks; set once by the worker thread at start.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn bind_current_thread(&self) {
        if let Parker::Thread(cell) = &self.parker {
            let _ = cell.set(std::thread::current());
        }
    }

    /// Queues `key` to run and wakes the worker if it is parked.
    pub(crate) fn schedule(&self, key: Key) {
        self.ready.push(key.to_bits());
        self.notify();
    }

    /// The next task to run.
    pub(crate) fn pop(&self) -> Option<Key> {
        self.ready.pop().map(Key::from_bits)
    }

    pub(crate) fn has_ready(&self) -> bool {
        !self.ready.is_empty()
    }

    /// Wakes the worker if it is parked; a running worker is left alone and
    /// sees the work at its next look. Call after publishing the work.
    pub(crate) fn notify(&self) {
        // Pairs with the fence in `parking`: either the worker's re-check sees
        // the work, or this load sees PARKED.
        fence(Ordering::SeqCst);
        if self.state.load(Ordering::Relaxed) == PARKED
            && self
                .state
                .compare_exchange(PARKED, NOTIFIED, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        {
            self.parker.unpark();
        }
    }

    /// The worker is about to park. It must look for work once more after
    /// this, and call [`running`](Self::running) when it goes on instead.
    pub(crate) fn parking(&self) {
        self.state.store(PARKED, Ordering::Relaxed);
        fence(Ordering::SeqCst);
    }

    /// The worker runs again.
    pub(crate) fn running(&self) {
        self.state.store(RUNNING, Ordering::Relaxed);
    }
}

/// Waking a worker's [`Signal`] wakes the worker: the waker a hook gets before
/// the worker parks.
impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.notify();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.notify();
    }
}

/// A task's waker: puts the task back on its own worker's run queue, once
/// until it runs again, from any thread.
pub(crate) struct TaskWaker {
    signal: Arc<Signal>,
    key: Key,
    scheduled: AtomicBool,
}

impl TaskWaker {
    /// A waker for a task that is queued to run already.
    pub(crate) fn new(signal: Arc<Signal>, key: Key) -> Arc<TaskWaker> {
        Arc::new(TaskWaker {
            signal,
            key,
            scheduled: AtomicBool::new(true),
        })
    }

    /// The worker is about to poll the task: a wake from now on queues it again.
    pub(crate) fn clear(&self) {
        self.scheduled.store(false, Ordering::SeqCst);
    }
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if !self.scheduled.swap(true, Ordering::AcqRel) {
            self.signal.schedule(self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct Count(AtomicUsize);

    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn signal() -> (Arc<Signal>, Arc<Count>) {
        let count = Arc::new(Count(AtomicUsize::new(0)));
        (
            Arc::new(Signal::new(Parker::Waker(Waker::from(count.clone())))),
            count,
        )
    }

    #[test]
    fn a_running_worker_is_never_woken_and_a_parked_one_once() {
        let (signal, wakes) = signal();
        signal.notify();
        assert_eq!(wakes.0.load(Ordering::SeqCst), 0);
        signal.parking();
        signal.notify();
        signal.notify();
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
        signal.running();
        signal.notify();
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_task_is_queued_once_until_it_runs() {
        let (signal, _) = signal();
        let key = super::super::slab::Slab::<()>::new().next_key();
        let waker = Waker::from(TaskWaker::new(signal.clone(), key));
        waker.wake_by_ref();
        assert!(!signal.has_ready(), "already queued at spawn");
        let task = TaskWaker::new(signal.clone(), key);
        task.clear();
        let waker = Waker::from(task);
        waker.wake_by_ref();
        waker.wake_by_ref();
        assert_eq!(signal.pop(), Some(key));
        assert_eq!(signal.pop(), None);
    }
}
