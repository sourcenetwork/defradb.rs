//! Which CPUs the process's placed pools hold, so no two of them sit on one
//! core.
//!
//! One counter per logical CPU: how many live placed pools hold it. A pool
//! that fits on the free CPUs takes them alone, all or nothing ([`Ledger::hold`]:
//! each counter moves from 0 to 1, and a CPU found held rolls back the ones
//! already taken); a pool that does not fit shares its CPUs
//! ([`Ledger::share`]) and says so in its report, so no later pool mistakes
//! them for free. A [`Hold`] gives its CPUs back when dropped. Lock-free:
//! one atomic per CPU touched, and nothing waits.
//!
//! A pool that starts while another one rolls back a failed claim may see a
//! few CPUs held that are free a moment later and plan on fewer; two placed
//! pools never end up on one CPU through the ledger.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::topology::{CpuSet, MAX_CPUS};

/// The process's ledger: zero pages until a pool holds a CPU.
static PROCESS: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];

/// A set of per-CPU hold counters: the process's, or a test's own.
#[derive(Clone, Copy)]
pub(crate) struct Ledger<'a> {
    holds: &'a [AtomicU32],
}

/// CPUs a pool holds; dropping it gives them back.
pub(crate) struct Hold<'a> {
    holds: &'a [AtomicU32],
    cpus: CpuSet,
}

impl Ledger<'static> {
    /// The ledger every pool of this process claims from.
    pub(crate) fn process() -> Self {
        Ledger { holds: &PROCESS }
    }
}

impl<'a> Ledger<'a> {
    /// A ledger over `holds`, one counter per CPU id: a test's own machine.
    #[cfg(test)]
    pub(crate) fn over(holds: &'a [AtomicU32]) -> Self {
        Ledger { holds }
    }

    /// The CPUs of `allowed` no pool holds.
    pub(crate) fn free(&self, allowed: &CpuSet) -> CpuSet {
        allowed
            .iter()
            .filter(|cpu| {
                self.holds
                    .get(*cpu)
                    .is_some_and(|held| held.load(Ordering::Acquire) == 0)
            })
            .collect()
    }

    /// `cpus` for one pool alone: `None`, with nothing taken, when another
    /// pool holds any of them.
    pub(crate) fn hold(&self, cpus: &CpuSet) -> Option<Hold<'a>> {
        let mut taken = CpuSet::new();
        for cpu in cpus.iter() {
            let won = self.holds.get(cpu).is_some_and(|held| {
                held.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            });
            if !won {
                drop(Hold {
                    holds: self.holds,
                    cpus: taken,
                });
                return None;
            }
            taken.insert(cpu);
        }
        Some(Hold {
            holds: self.holds,
            cpus: taken,
        })
    }

    /// `cpus` for a pool that shares them with the pools already there.
    pub(crate) fn share(&self, cpus: &CpuSet) -> Hold<'a> {
        let mut taken = CpuSet::new();
        for cpu in cpus.iter() {
            if let Some(held) = self.holds.get(cpu) {
                held.fetch_add(1, Ordering::AcqRel);
                taken.insert(cpu);
            }
        }
        Hold {
            holds: self.holds,
            cpus: taken,
        }
    }
}

#[cfg(test)]
impl Hold<'_> {
    /// The CPUs held.
    pub(crate) fn cpus(&self) -> &CpuSet {
        &self.cpus
    }
}

impl Drop for Hold<'_> {
    fn drop(&mut self) {
        for cpu in self.cpus.iter() {
            if let Some(held) = self.holds.get(cpu) {
                held.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }
}

impl std::fmt::Debug for Hold<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hold({})", self.cpus)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A ledger of its own, for a test's synthetic machine.
    pub(crate) fn ledger() -> Ledger<'static> {
        let holds: Vec<AtomicU32> = (0..256).map(|_| AtomicU32::new(0)).collect();
        Ledger::over(Box::leak(holds.into_boxed_slice()))
    }

    fn set(list: &str) -> CpuSet {
        CpuSet::parse(list).unwrap()
    }

    #[test]
    fn a_hold_is_all_or_nothing_and_gives_its_cpus_back_when_dropped() {
        let ledger = ledger();
        let all = set("0-7");
        let first = ledger.hold(&set("0-3")).unwrap();
        assert_eq!(ledger.free(&all), set("4-7"));
        assert!(ledger.hold(&set("3-5")).is_none(), "CPU 3 is held");
        assert_eq!(
            ledger.free(&all),
            set("4-7"),
            "the failed claim took nothing"
        );
        drop(first);
        assert_eq!(ledger.free(&all), all);
    }

    #[test]
    fn a_shared_cpu_stays_held_until_every_holder_is_gone() {
        let ledger = ledger();
        let alone = ledger.hold(&set("0-1")).unwrap();
        let shared = ledger.share(&set("1-2"));
        drop(alone);
        assert_eq!(ledger.free(&set("0-3")), set("0,3"));
        assert!(ledger.hold(&set("2")).is_none());
        drop(shared);
        assert_eq!(ledger.free(&set("0-3")), set("0-3"));
    }

    #[test]
    fn concurrent_claims_never_hold_one_cpu_twice() {
        let ledger = ledger();
        let all = set("0-63");
        // Overlapping windows of six CPUs, claimed by eight threads at once;
        // every hold is kept until all threads are done.
        let winners: Vec<Hold<'static>> = std::thread::scope(|scope| {
            let claims: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(move || {
                        (0..64)
                            .step_by(4)
                            .filter_map(|start| {
                                let cpus: CpuSet = (start..(start + 6).min(64)).collect();
                                ledger.hold(&cpus)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            claims
                .into_iter()
                .flat_map(|claim| claim.join().unwrap())
                .collect()
        });
        assert!(!winners.is_empty());
        let mut seen = CpuSet::new();
        for hold in &winners {
            let cpus = hold.cpus();
            assert!(seen.intersection(cpus).is_empty(), "{cpus} held twice");
            seen.union_with(cpus);
        }
        assert!(ledger.free(&all).intersection(&seen).is_empty());
        drop(winners);
        assert_eq!(ledger.free(&all), all, "every hold was given back");
    }
}
