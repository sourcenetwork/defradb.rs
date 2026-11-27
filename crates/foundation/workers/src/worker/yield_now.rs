//! Cutting long work into slices, so the other tasks on the core run: a
//! one-time yield, and a work budget that yields each time it is spent.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Lets every task already runnable on this worker run before the caller
/// goes on. Works on any executor: it wakes itself and returns `Pending` once.
pub fn yield_now() -> YieldNow {
    YieldNow { yielded: false }
}

/// The future [`yield_now`] returns.
#[must_use = "futures do nothing unless awaited"]
pub struct YieldNow {
    yielded: bool,
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            return Poll::Ready(());
        }
        self.yielded = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// A slice of work between yields: long CPU work spends units as it goes
/// (rows, bytes, pages) and yields to the worker each time a slice is used up.
///
/// ```
/// # async fn scan(rows: &[u64]) -> u64 {
/// let mut budget = workers::Budget::new(1024);
/// let mut sum = 0;
/// for row in rows {
///     sum += row;
///     budget.spend(1).await;
/// }
/// sum
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct Budget {
    slice: u32,
    left: u32,
}

impl Budget {
    /// A budget that yields after every `slice` units (at least one).
    pub fn new(slice: u32) -> Budget {
        let slice = slice.max(1);
        Budget { slice, left: slice }
    }

    /// Spends `units`; yields once the slice is used up and starts a new one.
    pub async fn spend(&mut self, units: u32) {
        if self.take(units) {
            yield_now().await;
        }
    }

    /// Spends `units` without yielding; `true` when the slice is used up and
    /// the caller should yield (a synchronous loop that cannot await).
    pub fn take(&mut self, units: u32) -> bool {
        self.left = self.left.saturating_sub(units);
        if self.left > 0 {
            return false;
        }
        self.left = self.slice;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_budget_asks_for_a_yield_once_per_slice() {
        let mut budget = Budget::new(3);
        let yields: Vec<bool> = (0..7).map(|_| budget.take(1)).collect();
        assert_eq!(yields, vec![false, false, true, false, false, true, false]);
        assert!(Budget::new(0).take(1), "a zero slice is one unit");
        assert!(Budget::new(10).take(25), "overspending yields once");
    }
}
