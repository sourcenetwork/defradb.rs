use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::{Error, Result};

/// Maximum active expression evaluations in one permission traversal.
pub const MAX_EVALUATION_DEPTH: usize = 64;
/// Maximum expression visits and relationship candidates per check or batch.
pub const MAX_EVALUATION_STEPS: usize = 10_000;

#[derive(Debug, Default)]
pub(crate) struct EvaluationBudget {
    steps: AtomicUsize,
    depth: AtomicUsize,
}

impl EvaluationBudget {
    pub(crate) fn charge(&self) -> Result<()> {
        self.steps
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |steps| {
                (steps < MAX_EVALUATION_STEPS).then_some(steps + 1)
            })
            .map(|_| ())
            .map_err(|_| Error::EvaluationLimitExceeded("steps"))
    }

    pub(crate) fn enter(&self) -> Result<EvaluationGuard<'_>> {
        self.charge()?;
        if self.depth.fetch_add(1, Ordering::Relaxed) >= MAX_EVALUATION_DEPTH {
            self.depth.fetch_sub(1, Ordering::Relaxed);
            return Err(Error::EvaluationLimitExceeded("depth"));
        }
        Ok(EvaluationGuard(self))
    }
}

pub(crate) struct EvaluationGuard<'a>(&'a EvaluationBudget);

impl Drop for EvaluationGuard<'_> {
    fn drop(&mut self) {
        self.0.depth.fetch_sub(1, Ordering::Relaxed);
    }
}
