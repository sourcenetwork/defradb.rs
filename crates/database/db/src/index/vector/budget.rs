//! The node's budget for what vector indexes keep in memory between
//! searches: DiskANN's resident codes, the codes a search fills in on a miss,
//! and the copies of pending lists. This is a steady-state bound: a reload may
//! briefly overlap its old and new generations while in-flight searches finish.
//!
//! One budget per node, shared by every worker. Taking room is one
//! compare-and-swap on a counter, so no worker waits on another. A cache that
//! is refused room is not kept, and the searches that would have read it read
//! the same bytes from storage instead: the budget changes how fast a search
//! is, never what it returns. Modeled in `proofs/tla/VectorMemoryBudget.tla`;
//! the accounting is proved in `proofs/lean/VectorMemoryBudget.lean`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// The budget a node gets when its configuration names none: room for the
/// resident codes of about two million 96-byte codes, on a quarter of a
/// gigabyte. This keeps an unbounded vector index from consuming the whole
/// node by default; a larger existing index reads overflow codes from storage
/// until an operator raises `--vector-memory-budget`.
pub const DEFAULT_VECTOR_MEMORY_BUDGET: u64 = 256 << 20;

/// Bytes the caches of one node may hold, and the bytes they hold now.
#[derive(Debug)]
pub struct MemoryBudget {
    limit: u64,
    used: AtomicU64,
    refusals: AtomicU64,
    warned: AtomicBool,
}

impl MemoryBudget {
    pub fn new(limit: u64) -> Arc<Self> {
        let budget = Arc::new(Self {
            limit,
            used: AtomicU64::new(0),
            refusals: AtomicU64::new(0),
            warned: AtomicBool::new(false),
        });
        let source: Arc<dyn telemetry::VectorMemoryBudgetSource> = budget.clone();
        telemetry::register_vector_memory_budget(Arc::downgrade(&source));
        budget
    }

    /// A budget nothing is refused from, for an index used outside a node.
    pub fn unlimited() -> Arc<Self> {
        Self::new(u64::MAX)
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// The bytes every live [`Charge`] holds.
    pub fn used(&self) -> u64 {
        self.used.load(Ordering::Acquire)
    }

    /// Charge attempts refused because the budget was full.
    pub fn refusals(&self) -> u64 {
        self.refusals.load(Ordering::Acquire)
    }

    /// A charge of `bytes`, or `None` when they do not fit.
    pub fn charge(self: &Arc<Self>, bytes: u64) -> Option<Charge> {
        let charge = Charge::empty(self);
        charge.grow(bytes).then_some(charge)
    }

    /// Takes `bytes` if they fit beside what is held: the check and the take
    /// are one step, so two takers never both fit into the same room.
    fn take(&self, bytes: u64) -> bool {
        let mut used = self.used.load(Ordering::Acquire);
        loop {
            let Some(total) = used.checked_add(bytes).filter(|total| *total <= self.limit) else {
                self.refuse(used, bytes);
                return false;
            };
            match self
                .used
                .compare_exchange_weak(used, total, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return true,
                Err(current) => used = current,
            }
        }
    }

    /// Replaces `released` bytes held by one cache copy with `bytes` held by
    /// its replacement, in one compare-and-swap.
    pub(super) fn replace(&self, released: u64, bytes: u64) -> bool {
        let mut used = self.used.load(Ordering::Acquire);
        loop {
            let Some(total) = used
                .checked_sub(released)
                .and_then(|base| base.checked_add(bytes))
                .filter(|total| *total <= self.limit)
            else {
                self.refuse(used, bytes);
                return false;
            };
            match self
                .used
                .compare_exchange_weak(used, total, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return true,
                Err(current) => used = current,
            }
        }
    }

    fn refuse(&self, used: u64, bytes: u64) {
        self.refusals.fetch_add(1, Ordering::Relaxed);
        if !self.warned.swap(true, Ordering::AcqRel) {
            tracing::warn!(
                limit = self.limit,
                used,
                requested = bytes,
                "vector memory budget refused room; overflow is read from storage until room frees or --vector-memory-budget is raised"
            );
        }
    }

    fn give(&self, bytes: u64) {
        self.used.fetch_sub(bytes, Ordering::AcqRel);
    }

    pub(super) fn restore(&self, bytes: u64) {
        if bytes > 0 {
            self.used.fetch_add(bytes, Ordering::AcqRel);
        }
    }
}

/// Bytes held against a [`MemoryBudget`], given back when the charge is
/// released or dropped, whichever comes first.
#[derive(Debug)]
pub struct Charge {
    budget: Arc<MemoryBudget>,
    bytes: AtomicU64,
}

impl Charge {
    /// A charge holding nothing yet.
    pub fn empty(budget: &Arc<MemoryBudget>) -> Self {
        Self {
            budget: budget.clone(),
            bytes: AtomicU64::new(0),
        }
    }

    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Acquire)
    }

    pub(super) fn reserved(budget: &Arc<MemoryBudget>, bytes: u64) -> Self {
        Self {
            budget: budget.clone(),
            bytes: AtomicU64::new(bytes),
        }
    }

    /// Forgets bytes already removed from the budget by a replacement.
    pub(super) fn renounce(&self) {
        self.bytes.swap(0, Ordering::AcqRel);
    }

    /// Gives back `bytes` from this charge.
    pub(super) fn shrink(&self, bytes: u64) {
        let mut held = self.bytes.load(Ordering::Acquire);
        loop {
            let released = bytes.min(held);
            let next = held - released;
            match self
                .bytes
                .compare_exchange_weak(held, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    self.budget.give(released);
                    return;
                }
                Err(current) => held = current,
            }
        }
    }

    /// Takes `more` bytes, unless the budget has no room for them.
    pub fn grow(&self, more: u64) -> bool {
        if more == 0 {
            return true;
        }
        if !self.budget.take(more) {
            return false;
        }
        self.bytes.fetch_add(more, Ordering::AcqRel);
        true
    }

    /// Holds at least `target` bytes, taking `step` at a time where the
    /// budget has that much room, so filling a large cache is not one
    /// compare-and-swap per entry.
    pub fn cover(&mut self, target: u64, step: u64) -> bool {
        let held = *self.bytes.get_mut();
        if held >= target {
            return true;
        }
        let missing = target - held;
        self.grow(missing.max(step)) || self.grow(missing)
    }

    /// Holds exactly `target` bytes: gives back what is held past it, or
    /// takes what is missing, unless the budget has no room for it.
    pub fn settle(&mut self, target: u64) -> bool {
        let held = *self.bytes.get_mut();
        if held > target {
            *self.bytes.get_mut() = target;
            self.budget.give(held - target);
            return true;
        }
        self.grow(target - held)
    }

    /// Gives back everything held. Bytes taken afterwards are given back on
    /// drop.
    pub fn release(&self) {
        let held = self.bytes.swap(0, Ordering::AcqRel);
        if held > 0 {
            self.budget.give(held);
        }
    }
}

impl Drop for Charge {
    fn drop(&mut self) {
        self.release();
    }
}

impl telemetry::VectorMemoryBudgetSource for MemoryBudget {
    fn vector_memory_budget_snapshot(&self) -> telemetry::VectorMemoryBudgetSnapshot {
        telemetry::VectorMemoryBudgetSnapshot {
            used: self.used(),
            limit: self.limit(),
            refusals: self.refusals(),
        }
    }
}
