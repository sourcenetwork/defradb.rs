//! A worker's deadline queue: timers and periodic jobs on the pool's clock,
//! fired between task polls. A cancelled timer leaves a stale heap entry; the
//! heap is rebuilt once stale entries outnumber live ones, so it stays within
//! twice the live timers.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use super::slab::{Key, Slab};
use super::{micros, Local};
use crate::Error;

/// Rebuild the heap only past this many stale entries.
const COMPACT_AFTER: usize = 64;

pub(crate) struct Timers {
    heap: BinaryHeap<Reverse<(u64, u64)>>,
    wakers: Slab<Waker>,
    stale: usize,
}

impl Timers {
    pub(crate) fn new() -> Timers {
        Timers {
            heap: BinaryHeap::new(),
            wakers: Slab::new(),
            stale: 0,
        }
    }

    pub(crate) fn add(&mut self, due: u64, waker: Waker) -> Key {
        let key = self.wakers.insert(waker);
        self.heap.push(Reverse((due, key.to_bits())));
        key
    }

    /// Points a pending timer at `waker`; `false` when it already fired.
    pub(crate) fn update(&mut self, key: Key, waker: &Waker) -> bool {
        match self.wakers.get_mut(key) {
            Some(held) => {
                if !held.will_wake(waker) {
                    held.clone_from(waker);
                }
                true
            }
            None => false,
        }
    }

    pub(crate) fn cancel(&mut self, key: Key) {
        if self.wakers.remove(key).is_none() {
            return;
        }
        self.stale += 1;
        if self.stale > COMPACT_AFTER && self.stale * 2 > self.heap.len() {
            let wakers = &self.wakers;
            self.heap
                .retain(|Reverse((_, bits))| wakers.contains(Key::from_bits(*bits)));
            self.stale = 0;
        }
    }

    /// Moves the wakers of every timer due at `now` into `woken`.
    pub(crate) fn fire(&mut self, now: u64, woken: &mut Vec<Waker>) {
        while let Some(&Reverse((due, bits))) = self.heap.peek() {
            if due > now {
                break;
            }
            self.heap.pop();
            match self.wakers.remove(Key::from_bits(bits)) {
                Some(waker) => woken.push(waker),
                None => self.stale = self.stale.saturating_sub(1),
            }
        }
    }

    /// When the earliest pending timer is due.
    pub(crate) fn next_due(&mut self) -> Option<u64> {
        while let Some(&Reverse((due, bits))) = self.heap.peek() {
            if self.wakers.contains(Key::from_bits(bits)) {
                return Some(due);
            }
            self.heap.pop();
            self.stale = self.stale.saturating_sub(1);
        }
        None
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.wakers.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.heap.clear();
        drop(self.wakers.drain());
        self.stale = 0;
    }

    #[cfg(test)]
    pub(crate) fn pending(&self) -> usize {
        self.wakers.len()
    }

    #[cfg(test)]
    pub(crate) fn heap_len(&self) -> usize {
        self.heap.len()
    }
}

/// Completes once `duration` has passed on the pool's clock. Only on a worker
/// thread: the timer lives in that worker's deadline queue.
pub fn sleep(duration: Duration) -> Result<Sleep, Error> {
    let local = super::current().ok_or(Error::NotOnWorker)?;
    let due = local.now().saturating_add(micros(duration));
    Ok(Sleep {
        local,
        due,
        key: None,
    })
}

/// The future [`sleep`] returns.
#[must_use = "futures do nothing unless awaited"]
pub struct Sleep {
    local: Rc<Local>,
    due: u64,
    key: Option<Key>,
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.local.now() >= this.due {
            if let Some(key) = this.key.take() {
                this.local.timers.borrow_mut().cancel(key);
            }
            return Poll::Ready(());
        }
        let mut timers = this.local.timers.borrow_mut();
        let registered = this.key.is_some_and(|key| timers.update(key, cx.waker()));
        if !registered {
            this.key = Some(timers.add(this.due, cx.waker().clone()));
        }
        Poll::Pending
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        if let (Some(key), Ok(mut timers)) = (self.key.take(), self.local.timers.try_borrow_mut()) {
            timers.cancel(key);
        }
    }
}

/// Runs `job` every `period` on the calling worker, forever: spawn it as a
/// task (`spawn_daemon` for work shutdown should not wait for). Fails only off
/// a worker.
///
/// ```no_run
/// # async fn demo() -> Result<(), workers::Error> {
/// workers::spawn_daemon(async {
///     let _ = workers::every(std::time::Duration::from_secs(1), || async {
///         // the idle-transaction sweep, a metrics flush, ...
///     })
///     .await;
/// })?;
/// # Ok(())
/// # }
/// ```
pub async fn every<F, Fut>(period: Duration, mut job: F) -> Result<std::convert::Infallible, Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()>,
{
    loop {
        sleep(period)?.await;
        job().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::task::Wake;

    struct Count(AtomicUsize);

    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn fires_in_due_order_and_skips_cancelled() {
        let count = Arc::new(Count(AtomicUsize::new(0)));
        let waker = Waker::from(count.clone());
        let mut timers = Timers::new();
        let late = timers.add(30, waker.clone());
        let early = timers.add(10, waker.clone());
        let cancelled = timers.add(20, waker.clone());
        timers.cancel(cancelled);
        assert_eq!(timers.next_due(), Some(10));
        let mut woken = Vec::new();
        timers.fire(25, &mut woken);
        assert_eq!(woken.len(), 1);
        assert!(!timers.update(early, &waker), "fired");
        assert!(timers.update(late, &waker));
        assert_eq!(timers.next_due(), Some(30));
        woken.clear();
        timers.fire(30, &mut woken);
        assert_eq!(woken.len(), 1);
        assert!(timers.is_empty());
        assert_eq!(timers.next_due(), None);
    }

    #[test]
    fn cancelled_timers_never_hold_the_heap_past_twice_the_live_ones() {
        let waker = Waker::from(Arc::new(Count(AtomicUsize::new(0))));
        let mut timers = Timers::new();
        let keep = timers.add(1_000_000, waker.clone());
        for due in 0..10_000 {
            let key = timers.add(due, waker.clone());
            timers.cancel(key);
        }
        assert!(
            timers.heap_len() <= 2 * COMPACT_AFTER + 2,
            "heap {}",
            timers.heap_len()
        );
        assert!(timers.update(keep, &waker));
    }
}
