//! A counting semaphore whose waiters are served in arrival order.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use thiserror::Error;

use super::OneThread;

/// The semaphore was closed.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("semaphore closed")]
pub struct AcquireError(());

/// Why [`Semaphore::try_acquire`] gave no permit.
#[derive(Debug, Error, PartialEq, Eq, Clone, Copy)]
pub enum TryAcquireError {
    /// The semaphore was closed.
    #[error("semaphore closed")]
    Closed,
    /// No permit is available, or others are waiting ahead.
    #[error("no permits available")]
    NoPermits,
}

#[derive(Debug, Default)]
struct State {
    permits: usize,
    closed: bool,
    next_id: u64,
    waiters: VecDeque<(u64, Waker)>,
}

impl State {
    fn wake_front(&self) {
        if self.permits > 0 {
            if let Some((_, waker)) = self.waiters.front() {
                waker.wake_by_ref();
            }
        }
    }
}

/// A counting semaphore, served first come first served.
pub struct Semaphore {
    state: OneThread<RefCell<State>>,
}

impl std::fmt::Debug for Semaphore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Semaphore")
            .field("permits", &self.available_permits())
            .finish_non_exhaustive()
    }
}

impl Semaphore {
    /// A semaphore with `permits` permits.
    pub fn new(permits: usize) -> Self {
        Self {
            state: OneThread(RefCell::new(State {
                permits,
                ..State::default()
            })),
        }
    }

    /// Permits free right now.
    pub fn available_permits(&self) -> usize {
        self.state.borrow().permits
    }

    /// Adds `n` permits and wakes the first waiter.
    pub fn add_permits(&self, n: usize) {
        let mut state = self.state.borrow_mut();
        state.permits = state.permits.saturating_add(n);
        state.wake_front();
    }

    /// Fails every waiting and future acquire.
    pub(crate) fn close(&self) {
        let mut state = self.state.borrow_mut();
        state.closed = true;
        for (_, waker) in state.waiters.drain(..) {
            waker.wake();
        }
    }

    fn take_now(&self) -> Result<(), TryAcquireError> {
        let mut state = self.state.borrow_mut();
        if state.closed {
            return Err(TryAcquireError::Closed);
        }
        if state.permits == 0 || !state.waiters.is_empty() {
            return Err(TryAcquireError::NoPermits);
        }
        state.permits -= 1;
        Ok(())
    }

    /// A permit if one is free and nobody waits ahead.
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, TryAcquireError> {
        self.take_now()
            .map(|()| SemaphorePermit { semaphore: self })
    }

    /// [`try_acquire`](Self::try_acquire) for a shared semaphore.
    pub fn try_acquire_owned(self: Arc<Self>) -> Result<OwnedSemaphorePermit, TryAcquireError> {
        self.take_now().map(|()| OwnedSemaphorePermit {
            semaphore: self,
            forgotten: false,
        })
    }

    /// Waits for a permit.
    pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, AcquireError> {
        Acquire::new(self).await?;
        Ok(SemaphorePermit { semaphore: self })
    }

    /// [`acquire`](Self::acquire) for a shared semaphore.
    pub async fn acquire_owned(self: Arc<Self>) -> Result<OwnedSemaphorePermit, AcquireError> {
        Acquire::new(&self).await?;
        Ok(OwnedSemaphorePermit {
            semaphore: self,
            forgotten: false,
        })
    }
}

struct Acquire<'a> {
    semaphore: &'a Semaphore,
    id: Option<u64>,
}

impl<'a> Acquire<'a> {
    fn new(semaphore: &'a Semaphore) -> Self {
        Self {
            semaphore,
            id: None,
        }
    }
}

impl Future for Acquire<'_> {
    type Output = Result<(), AcquireError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.semaphore.state.borrow_mut();
        if state.closed {
            drop(state);
            self.id = None;
            return Poll::Ready(Err(AcquireError(())));
        }
        let id = match self.id {
            Some(id) => id,
            None => {
                let id = state.next_id;
                state.next_id += 1;
                id
            }
        };
        let front = state.waiters.front().is_none_or(|(front, _)| *front == id);
        if front && state.permits > 0 {
            state.permits -= 1;
            if self.id.is_some() {
                state.waiters.pop_front();
            }
            state.wake_front();
            drop(state);
            self.id = None;
            return Poll::Ready(Ok(()));
        }
        match state.waiters.iter_mut().find(|(waiting, _)| *waiting == id) {
            Some((_, waker)) => waker.clone_from(cx.waker()),
            None => state.waiters.push_back((id, cx.waker().clone())),
        }
        drop(state);
        self.id = Some(id);
        Poll::Pending
    }
}

impl Drop for Acquire<'_> {
    fn drop(&mut self) {
        let Some(id) = self.id else { return };
        let mut state = self.semaphore.state.borrow_mut();
        state.waiters.retain(|(waiting, _)| *waiting != id);
        state.wake_front();
    }
}

/// A permit borrowed from a [`Semaphore`]; dropping it returns it.
#[derive(Debug)]
pub struct SemaphorePermit<'a> {
    semaphore: &'a Semaphore,
}

impl SemaphorePermit<'_> {
    /// Keeps the permit out of the semaphore for good.
    pub fn forget(self) {
        std::mem::forget(self);
    }
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.semaphore.add_permits(1);
    }
}

/// A permit that owns its semaphore; dropping it returns it.
#[derive(Debug)]
pub struct OwnedSemaphorePermit {
    semaphore: Arc<Semaphore>,
    forgotten: bool,
}

impl OwnedSemaphorePermit {
    /// Keeps the permit out of the semaphore for good.
    pub fn forget(mut self) {
        self.forgotten = true;
    }
}

impl Drop for OwnedSemaphorePermit {
    fn drop(&mut self) {
        if !self.forgotten {
            self.semaphore.add_permits(1);
        }
    }
}
