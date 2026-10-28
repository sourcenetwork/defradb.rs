//! The network edge's own cores: whole cores no other holder has, held alone
//! in the process's [`Ledger`] for as long as the edge's threads run, so no
//! pool plans a worker on them, and a pool sets nothing aside for the edge
//! while one is held. The edge claims them the way a pool claims alone,
//! under the same cap on lost claims; it never shares, and with no claim won
//! its threads run unpinned and it holds nothing.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::core_order;
use super::reserve::ATTEMPTS;
use crate::ledger::{Hold, Ledger};
use crate::os;
use crate::topology::{CoreKind, CpuSet, Topology};

/// Network runtimes holding cores of their own.
static HOLDING: AtomicUsize = AtomicUsize::new(0);

/// Whether a network runtime holds cores of its own.
pub(crate) fn holding() -> bool {
    HOLDING.load(Ordering::Acquire) > 0
}

/// The whole cores a network runtime sized from the machine runs on: a
/// third of the cores the process may use, at least one per node.
pub fn edge_cores(topology: &Topology) -> usize {
    let cores = topology.core_count();
    let usable = topology.cpu_quota.map_or(cores, |quota| cores.min(quota));
    (usable / 3).max(topology.nodes.len()).max(1)
}

/// The CPUs a network runtime's threads run on, and its hold on them.
pub struct EdgeCores {
    threads: usize,
    cpus: CpuSet,
    allowed: CpuSet,
    _held: Option<Held>,
}

/// A hold that counts in [`HOLDING`] while it lives.
struct Held(Option<Hold<'static>>);

impl Drop for Held {
    fn drop(&mut self) {
        drop(self.0.take());
        HOLDING.fetch_sub(1, Ordering::AcqRel);
    }
}

impl EdgeCores {
    /// The cores of a network runtime sized from this machine, held for it
    /// alone, with the calling thread pinned to them: call it on the thread
    /// that starts the runtime's threads, which inherit the pin. Unpinned,
    /// holding nothing, when no cores are free, when they would leave none
    /// for a worker, or when the OS refuses the pin.
    pub fn reserve() -> EdgeCores {
        let machine = Topology::detect();
        let cores = Self::reserve_on(&machine);
        if cores.cpus.is_empty() || cores.pin_current_thread() {
            return cores;
        }
        Self::sized(&machine)
    }

    /// [`reserve`](Self::reserve) on `machine`, without pinning the caller.
    pub fn reserve_on(machine: &Topology) -> EdgeCores {
        let ledger = Ledger::process();
        let allowed = machine.allowed();
        for _ in 0..ATTEMPTS {
            let free = ledger.free(&allowed);
            let held = allowed.len() - free.len();
            let rest = Topology {
                cpu_quota: machine.cpu_quota.map(|quota| quota.saturating_sub(held)),
                ..machine.within(&free)
            };
            let Some(cpus) = choose(&rest) else {
                break;
            };
            if let Some(hold) = ledger.hold(&cpus) {
                HOLDING.fetch_add(1, Ordering::AcqRel);
                return EdgeCores {
                    threads: cpus.len(),
                    cpus,
                    allowed,
                    _held: Some(Held(Some(hold))),
                };
            }
        }
        Self::sized(machine)
    }

    /// `threads` unpinned threads, holding nothing.
    pub fn unpinned(threads: NonZeroUsize) -> EdgeCores {
        EdgeCores {
            threads: threads.get(),
            cpus: CpuSet::new(),
            allowed: CpuSet::new(),
            _held: None,
        }
    }

    /// As many unpinned threads as `machine`'s edge cores have CPUs.
    fn sized(machine: &Topology) -> EdgeCores {
        let cores = machine.core_count().max(1);
        let threads = (machine.cpu_count() * edge_cores(machine) / cores).max(1);
        EdgeCores {
            threads,
            cpus: CpuSet::new(),
            allowed: CpuSet::new(),
            _held: None,
        }
    }

    /// How many threads the runtime runs: one per CPU held, or as asked.
    pub fn threads(&self) -> usize {
        self.threads
    }

    /// The CPUs held; empty when the threads run unpinned.
    pub fn cpus(&self) -> &CpuSet {
        &self.cpus
    }

    /// Whether the threads run on cores of their own.
    pub fn is_pinned(&self) -> bool {
        !self.cpus.is_empty()
    }

    /// Pins the calling thread to the CPUs held; `false` when there are none
    /// or the OS refused.
    pub fn pin_current_thread(&self) -> bool {
        !self.cpus.is_empty()
            && os::pin(os::system(), &self.cpus, &self.allowed, 0)
                .cpus
                .is_some()
    }
}

/// [`edge_cores`] whole cores of `rest`: efficiency cores first where the
/// platform names them, then from the far end of the order workers take
/// cores in, one node after another, so the workers' first cores stay
/// theirs. `None` when that would leave no core for a worker.
fn choose(rest: &Topology) -> Option<CpuSet> {
    let wanted = edge_cores(rest);
    if wanted >= rest.core_count() {
        return None;
    }
    let mut order = core_order(rest, true);
    order.reverse();
    order.sort_by_key(|(_, core)| core.kind != CoreKind::Efficiency);
    let mut cpus = CpuSet::new();
    order
        .iter()
        .take(wanted)
        .for_each(|(_, core)| cpus.union_with(&core.cpus));
    Some(cpus)
}
