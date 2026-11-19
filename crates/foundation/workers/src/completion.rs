//! A task's result, delivered once and awaitable from anywhere: a tokio task,
//! another worker, or a plain thread through [`block_on`]. Built on a
//! `regolith::sync::Event`, so waiting needs no runtime and holds no lock.

use std::cell::UnsafeCell;
use std::future::Future;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use regolith::sync::{Event, EventWait};

use crate::Error;

const EMPTY: u8 = 0;
const WRITING: u8 = 1;
const FULL: u8 = 2;
const TAKEN: u8 = 3;
/// The waiter gave up before a result came: a task that has not started
/// yet never runs, and a result that comes is dropped.
const CANCELLED: u8 = 4;

/// The slot a result is written into once and taken out of once.
pub(crate) struct Shared<T> {
    event: Event,
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<Result<T, Error>>>,
}

// SAFETY: the value moves between threads (written by one, taken by
// another) but is never shared by reference: `state` hands it from the one
// writer to the one taker.
unsafe impl<T> Send for Shared<T> where T: Send {}
// SAFETY: as above.
unsafe impl<T> Sync for Shared<T> where T: Send {}

impl<T> Shared<T> {
    /// Stores the result unless one is stored already; `false` then.
    fn put(&self, value: Result<T, Error>) -> bool {
        if self
            .state
            .compare_exchange(EMPTY, WRITING, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return false;
        }
        // SAFETY: winning EMPTY -> WRITING makes this the only writer, and no
        // taker reads before FULL.
        unsafe { (*self.value.get()).write(value) };
        self.state.store(FULL, Ordering::Release);
        self.event.set();
        true
    }

    /// Gives up on the result unless it is in already.
    fn cancel(&self) {
        let _ = self
            .state
            .compare_exchange(EMPTY, CANCELLED, Ordering::AcqRel, Ordering::Relaxed);
    }

    fn take(&self) -> Option<Result<T, Error>> {
        self.state
            .compare_exchange(FULL, TAKEN, Ordering::Acquire, Ordering::Relaxed)
            .ok()?;
        // SAFETY: FULL -> TAKEN is won once, after the write completed.
        Some(unsafe { (*self.value.get()).assume_init_read() })
    }
}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        if *self.state.get_mut() == FULL {
            // SAFETY: FULL means written and not taken.
            unsafe { self.value.get_mut().assume_init_drop() };
        }
    }
}

/// Fails a completion, whatever its type: how the worker reports a task
/// that panicked.
pub(crate) trait Fail {
    fn fail(&self, error: Error);
}

impl<T> Fail for Shared<T> {
    fn fail(&self, error: Error) {
        self.put(Err(error));
    }
}

/// The sending side, held by the task. Dropping it unsent fails the
/// completion with [`Error::ShutDown`]: the pool dropped the task.
pub(crate) struct Sender<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Sender<T> {
    pub(crate) fn send(self, value: T) {
        self.shared.put(Ok(value));
    }

    pub(crate) fn shared(&self) -> Arc<Shared<T>> {
        self.shared.clone()
    }

    /// Whether the waiter gave up before the task started.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.shared.state.load(Ordering::Acquire) == CANCELLED
    }

    /// The sender's slot, without the sender's drop. A task's future holds
    /// the slot and the task itself settles what the future never sent (its
    /// panic, or `ShutDown` when dropped unfinished), so a panic unwinding
    /// through the future cannot settle the slot before the panic is
    /// reported.
    pub(crate) fn into_slot(self) -> Slot<T> {
        let this = std::mem::ManuallyDrop::new(self);
        // SAFETY: `this` is never dropped, so its one Arc is moved out once.
        Slot(unsafe { std::ptr::read(&this.shared) })
    }
}

/// Where a task's future puts its output ([`Sender::into_slot`]).
pub(crate) struct Slot<T>(Arc<Shared<T>>);

impl<T> Slot<T> {
    pub(crate) fn send(self, value: T) {
        self.0.put(Ok(value));
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.shared.put(Err(Error::ShutDown));
    }
}

/// A new completion and the sender that fills it.
pub(crate) fn channel<T>() -> (Sender<T>, Completion<T>) {
    let shared = Arc::new(Shared {
        event: Event::new(),
        state: AtomicU8::new(EMPTY),
        value: UnsafeCell::new(MaybeUninit::uninit()),
    });
    (
        Sender {
            shared: shared.clone(),
        },
        Completion { wait: None, shared },
    )
}

/// A task's result: `Ok` with its output, or why it has none (it panicked,
/// or the pool shut down first). Await it from any runtime or thread; a
/// plain thread can [`wait`](Completion::wait). Dropping it lets the task run
/// on, its result discarded.
#[must_use = "a completion does nothing unless awaited; drop it to detach the task"]
pub struct Completion<T> {
    // Declared before `shared`, so it is dropped first: it borrows the event
    // inside `shared`'s allocation.
    wait: Option<EventWait<'static>>,
    shared: Arc<Shared<T>>,
}

impl<T> Completion<T> {
    /// Whether the result is in.
    pub fn is_finished(&self) -> bool {
        self.shared.state.load(Ordering::Acquire) == FULL
    }

    /// Waits for the result on a plain thread. Errors with
    /// [`Error::OnWorker`] on a worker, which would wait on itself.
    #[cfg(not(target_family = "wasm"))]
    pub fn wait(self) -> Result<T, Error> {
        block_on(self)?
    }

    /// [`wait`](Self::wait) for at most `timeout`; [`Error::TimedOut`] after
    /// it, and the task runs on.
    #[cfg(not(target_family = "wasm"))]
    pub fn wait_timeout(self, timeout: std::time::Duration) -> Result<T, Error> {
        block_on_timeout(self, timeout)?
    }

    /// This completion awaited by a waiter that may give up: dropped before
    /// the result is in, a task that has not started never runs, and one
    /// already running runs to its end, its result discarded.
    pub fn cancel_on_drop(self) -> CancelOnDrop<T> {
        CancelOnDrop(self)
    }
}

/// A [`Completion`] whose task does not start once its waiter is gone
/// ([`Completion::cancel_on_drop`]).
#[must_use = "dropping it cancels a task that has not started"]
pub struct CancelOnDrop<T>(Completion<T>);

impl<T> Future for CancelOnDrop<T> {
    type Output = Result<T, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().0).poll(cx)
    }
}

impl<T> Drop for CancelOnDrop<T> {
    fn drop(&mut self) {
        self.0.shared.cancel();
    }
}

impl<T> Future for Completion<T> {
    type Output = Result<T, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(value) = this.shared.take() {
            this.wait = None;
            return Poll::Ready(value);
        }
        if matches!(this.shared.state.load(Ordering::Acquire), TAKEN | CANCELLED) {
            return Poll::Ready(Err(Error::Consumed));
        }
        let wait = this.wait.get_or_insert_with(|| {
            // SAFETY: the event lives in `shared`'s heap allocation, which
            // does not move and outlives this borrow: `shared` is held by
            // this completion, and `wait` is dropped before it (field order,
            // and cleared here on every return of the value).
            let event: &'static Event = unsafe { &*std::ptr::from_ref(&this.shared.event) };
            event.wait()
        });
        match Pin::new(wait).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(()) => {
                this.wait = None;
                Poll::Ready(this.shared.take().unwrap_or(Err(Error::Consumed)))
            }
        }
    }
}

impl<T> std::fmt::Debug for Completion<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Completion")
            .field("finished", &self.is_finished())
            .finish()
    }
}

/// Runs `future` to completion on this plain thread, parking between polls.
/// Errors with [`Error::OnWorker`] on a worker thread, where the wait would
/// stop the very thread that has to make progress.
#[cfg(not(target_family = "wasm"))]
pub fn block_on<F>(future: F) -> Result<F::Output, Error>
where
    F: Future,
{
    park_on(future, None)
}

/// [`block_on`] for at most `timeout`; [`Error::TimedOut`] after it.
#[cfg(not(target_family = "wasm"))]
pub fn block_on_timeout<F>(future: F, timeout: std::time::Duration) -> Result<F::Output, Error>
where
    F: Future,
{
    park_on(future, Some(std::time::Instant::now() + timeout))
}

#[cfg(not(target_family = "wasm"))]
fn park_on<F>(future: F, deadline: Option<std::time::Instant>) -> Result<F::Output, Error>
where
    F: Future,
{
    if crate::worker::on_worker() {
        return Err(Error::OnWorker);
    }
    let waker = std::task::Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return Ok(output);
        }
        match deadline {
            None => std::thread::park(),
            Some(deadline) => {
                let now = std::time::Instant::now();
                if now >= deadline {
                    return Err(Error::TimedOut);
                }
                std::thread::park_timeout(deadline - now);
            }
        }
    }
}

/// Wakes a thread parked in [`block_on`].
#[cfg(not(target_family = "wasm"))]
struct Unpark(std::thread::Thread);

#[cfg(not(target_family = "wasm"))]
impl std::task::Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_result_sent_from_another_thread_reaches_a_blocked_waiter() {
        let (sender, completion) = channel();
        let sender_thread = std::thread::spawn(move || sender.send(7));
        assert_eq!(completion.wait(), Ok(7));
        sender_thread.join().unwrap();
    }

    #[test]
    fn a_dropped_sender_fails_with_shut_down_and_a_failure_wins_over_it() {
        let (sender, completion) = channel::<u8>();
        drop(sender);
        assert_eq!(completion.wait(), Err(Error::ShutDown));

        let (sender, completion) = channel::<u8>();
        sender.shared().fail(Error::Panicked {
            task: "t",
            message: "boom".into(),
        });
        drop(sender);
        assert!(matches!(completion.wait(), Err(Error::Panicked { .. })));
    }

    #[test]
    fn a_timed_wait_gives_up_and_a_second_poll_says_consumed() {
        let (sender, completion) = channel::<u8>();
        assert_eq!(
            completion.wait_timeout(Duration::from_millis(1)),
            Err(Error::TimedOut)
        );
        drop(sender);

        let (sender, mut completion) = channel::<u8>();
        sender.send(1);
        assert!(completion.is_finished());
        assert_eq!(block_on(&mut completion), Ok(Ok(1)));
        assert_eq!(block_on(&mut completion), Ok(Err(Error::Consumed)));
    }

    #[test]
    fn a_cancelled_completion_takes_no_result_and_a_taken_one_stays_taken() {
        let (sender, completion) = channel::<u8>();
        drop(completion.cancel_on_drop());
        assert!(sender.is_cancelled());
        sender.send(1);

        let (sender, completion) = channel::<u8>();
        sender.send(2);
        let shared = Arc::clone(&completion.shared);
        let mut waiting = completion.cancel_on_drop();
        assert_eq!(block_on(&mut waiting), Ok(Ok(2)));
        drop(waiting);
        assert_eq!(shared.state.load(Ordering::Acquire), TAKEN);
    }

    #[test]
    fn an_unsent_value_is_dropped_with_the_slot() {
        let marker = Arc::new(());
        let (sender, completion) = channel();
        sender.send(marker.clone());
        drop(completion);
        assert_eq!(Arc::strong_count(&marker), 1);
    }

    #[test]
    fn completions_cross_threads() {
        fn send_sync<T>()
        where
            T: Send + Sync,
        {
        }
        send_sync::<Completion<Vec<u8>>>();
    }
}
