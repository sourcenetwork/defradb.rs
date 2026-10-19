use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A shared limit on saved cursor entries or their requested/discovered CIDs.
pub(super) struct Budget {
    pub(super) limit: usize,
    used: AtomicUsize,
}

impl Budget {
    pub(super) fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            limit,
            used: AtomicUsize::new(0),
        })
    }

    pub(super) fn reserve(self: &Arc<Self>, count: usize) -> Option<Reservation> {
        self.try_reserve(count).then(|| Reservation {
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

/// A charge is released when state leaves the idle cache.
pub(super) struct Reservation {
    budget: Arc<Budget>,
    count: usize,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.count, Ordering::Relaxed);
    }
}
