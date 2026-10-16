use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Counts requested and discovered CIDs across active and retained walks.
pub(super) struct CidBudget {
    pub(super) limit: usize,
    used: AtomicUsize,
}

impl CidBudget {
    pub(super) fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            limit,
            used: AtomicUsize::new(0),
        })
    }

    pub(super) fn reserve(self: &Arc<Self>, count: usize) -> Option<CidReservation> {
        self.try_reserve(count).then(|| CidReservation {
            budget: self.clone(),
            count,
        })
    }

    fn try_reserve(&self, count: usize) -> bool {
        let mut used = self.used.load(Ordering::Relaxed);
        loop {
            let Some(next) = used.checked_add(count).filter(|next| *next <= self.limit) else {
                return false;
            };
            match self
                .used
                .compare_exchange_weak(used, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(current) => used = current,
            }
        }
    }

    #[cfg(test)]
    pub(super) fn used(&self) -> usize {
        self.used.load(Ordering::Relaxed)
    }
}

/// The charge lives with the state, even after registry eviction or cancellation.
pub(super) struct CidReservation {
    budget: Arc<CidBudget>,
    count: usize,
}

impl CidReservation {
    pub(super) fn add(&mut self, count: usize) -> bool {
        if !self.budget.try_reserve(count) {
            return false;
        }
        self.count += count;
        true
    }
}

impl Drop for CidReservation {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.count, Ordering::Relaxed);
    }
}
