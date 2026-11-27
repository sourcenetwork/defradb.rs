//! Placing a thread: pin it, set its memory policy (after pinning, so local
//! means the node it now runs on), set its priority; and checking later that
//! nothing outside moved it, pinning it again if something did.
//!
//! Workers take the plan's seats. Every other class (edge, periodic,
//! compaction, blocking) takes the background CPUs, one node per thread in
//! turn. The placer keeps the pool's hold on its CPUs: they go back to the
//! ledger once the pool, its blocking threads and every env placing threads
//! through it are gone. A class's first fallback is warned about once; the
//! workers' are summed up in the start-up report instead.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::config::{MemoryPolicy, Priorities, ThreadClass};
#[cfg(not(target_family = "wasm"))]
use crate::ledger::Hold;
use crate::os::{self, Os, Outcome, Scheduling};
use crate::placement::Plan;
use crate::topology::CpuSet;
use crate::worker::{bump, WorkerShared};

/// What placing one thread came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadPlacement {
    /// The CPUs asked for and the ones the thread is verified to run on.
    pub pinning: Outcome,
    /// The NUMA memory policy.
    pub memory_policy: Outcome,
    /// The scheduling priority.
    pub priority: Outcome,
}

/// A placed thread's settings, kept to check them again later.
pub(crate) struct Placed {
    pub report: ThreadPlacement,
    pub cpus: Option<CpuSet>,
    pub scheduling: Option<Scheduling>,
    tag: usize,
}

pub(crate) struct Placer {
    pub plan: Plan,
    /// The process's allowed CPUs, where a failed pin goes back to.
    allowed: CpuSet,
    priorities: Priorities,
    memory_policy: MemoryPolicy,
    /// Threads placed so far per class, for the round robin over nodes.
    placed: [AtomicUsize; 5],
    /// Whether a class's fallback was warned about already.
    warned: [AtomicBool; 5],
    /// The CPUs the plan holds in the process's ledger, given back once
    /// nothing places threads through this placer any more.
    #[cfg(not(target_family = "wasm"))]
    _hold: Option<Hold<'static>>,
}

fn slot(class: ThreadClass) -> usize {
    ThreadClass::ALL
        .iter()
        .position(|c| *c == class)
        .unwrap_or(0)
}

impl Placer {
    pub(crate) fn new(
        plan: Plan,
        allowed: CpuSet,
        priorities: Priorities,
        memory_policy: MemoryPolicy,
    ) -> Placer {
        Placer {
            plan,
            allowed,
            priorities,
            memory_policy,
            placed: Default::default(),
            warned: Default::default(),
            #[cfg(not(target_family = "wasm"))]
            _hold: None,
        }
    }

    /// This placer, keeping `hold` for as long as it lives.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn holding(self, hold: Option<Hold<'static>>) -> Placer {
        Placer {
            _hold: hold,
            ..self
        }
    }

    /// Places the calling thread as worker `index`.
    pub(crate) fn place_worker(&self, index: usize) -> Placed {
        let cpus = self
            .plan
            .seats
            .get(index)
            .map(|seat| seat.cpus.clone())
            .unwrap_or_default();
        self.apply(os::system(), &cpus, index + 1, ThreadClass::Worker)
    }

    /// Places the calling thread as the next thread of `class`, on one node's
    /// background CPUs at the class's priority, and warns once per class if
    /// anything fell back.
    pub(crate) fn place(&self, class: ThreadClass) -> ThreadPlacement {
        let n = self.placed[slot(class)].fetch_add(1, Ordering::Relaxed);
        let (_, cpus) = self.plan.background_for(n);
        let placed = self.apply(os::system(), &cpus, 0, class);
        let report = placed.report;
        let fell_back = [&report.pinning, &report.memory_policy, &report.priority]
            .into_iter()
            .find_map(|outcome| outcome.fell_back.as_deref());
        if let Some(reason) = fell_back {
            if !self.warned[slot(class)].swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    class = class.name(),
                    pinning = %report.pinning,
                    memory_policy = %report.memory_policy,
                    priority = %report.priority,
                    reason,
                    "a thread setting fell back to a weaker one"
                );
            }
        }
        report
    }

    fn apply(&self, os: &dyn Os, cpus: &CpuSet, tag: usize, class: ThreadClass) -> Placed {
        let pinned = os::pin(os, cpus, &self.allowed, tag);
        let memory_policy = os::set_memory_policy(os, &self.memory_policy);
        let priority = os::prioritize(os, self.priorities.of(class));
        Placed {
            report: ThreadPlacement {
                pinning: pinned.outcome,
                memory_policy,
                priority,
            },
            cpus: pinned.cpus,
            scheduling: os.scheduling().ok(),
            tag,
        }
    }

    /// Checks the calling worker is still where `placed` put it: pinned to
    /// its CPUs and running on one, at the priority it was granted. A worker
    /// found off its CPUs is reported once and pinned again; a changed
    /// priority is reported once (an outside tool reset it).
    pub(crate) fn recheck(&self, placed: &Placed, worker: &WorkerShared, warned: &mut [bool; 2]) {
        let os = os::system();
        if let Some(cpus) = &placed.cpus {
            if let Err(reason) = os::check_pin(os, cpus) {
                bump(&worker.counters.repins);
                if !std::mem::replace(&mut warned[0], true) {
                    tracing::warn!(worker = worker.id.index, cpus = %cpus, reason, "a worker was moved off its CPUs; pinning it again");
                }
                let again = os::pin(os, cpus, &self.allowed, placed.tag);
                if again.cpus.is_none() {
                    tracing::warn!(worker = worker.id.index, outcome = %again.outcome, "pinning the worker again failed");
                }
            }
        }
        if let (Some(granted), Ok(now)) = (placed.scheduling, os.scheduling()) {
            if granted != now {
                bump(&worker.counters.priority_resets);
                if !std::mem::replace(&mut warned[1], true) {
                    tracing::warn!(
                        worker = worker.id.index,
                        granted = %granted,
                        now = %now,
                        "a worker's priority was changed from outside the pool"
                    );
                }
            }
        }
    }
}
