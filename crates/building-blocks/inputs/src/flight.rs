//! One key's round trip in progress: the answer, once, and the event its
//! waiters suspend on.

use regolith::sync::{Event, OnceCell};

use crate::error::Error;

/// A key's answer and the round trip that brought it.
pub(crate) struct Answer<V, E> {
    pub(crate) result: Result<V, Error<E>>,
    /// The round trip's number; 0 for an answer found in the cache.
    pub(crate) trip: u64,
}

/// One key in flight. Set once; every waiter reads the same answer.
pub(crate) struct Flight<V, E> {
    done: Event,
    answer: OnceCell<Answer<V, E>>,
    /// The cache generation when the flight began: its answer is cached only
    /// if no invalidation began since.
    pub(crate) generation: u64,
}

impl<V, E> Flight<V, E> {
    pub(crate) fn new(generation: u64) -> Self {
        Self {
            done: Event::new(),
            answer: OnceCell::new(),
            generation,
        }
    }

    /// Settles the flight. The first answer wins; later ones are dropped.
    pub(crate) fn settle(&self, answer: Answer<V, E>) {
        self.record(answer);
        self.release();
    }

    /// Records the answer without waking anyone yet.
    pub(crate) fn record(&self, answer: Answer<V, E>) {
        let _ = self.answer.set(answer);
    }

    /// Wakes every waiter; the answer recorded is theirs.
    pub(crate) fn release(&self) {
        self.done.set();
    }

    pub(crate) fn answer(&self) -> Option<&Answer<V, E>> {
        if self.done.is_set() {
            self.answer.get()
        } else {
            None
        }
    }

    pub(crate) async fn settled(&self) {
        self.done.wait().await;
    }
}
